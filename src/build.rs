//! A small toolchain harness that ties inferred-IR code generation to a C
//! compiler: generate the C, compile it, and run it.
//!
//! The three methods form a progression, each a superset of the previous:
//! [`CBuild::generate`] → C source, [`CBuild::compile`] → a compiled binary,
//! [`CBuild::run`] → the program's output.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::builder::InstructionBuilder;
use crate::codegen::{CodegenError, CompilerError, emit_body, emit_code};
use crate::instructions::Instr;
use crate::program::Program;
use crate::registers::RegisterFile;
use crate::solver::{Solver, TypeError};

/// Anything that can go wrong on the way from IR to a running program.
#[derive(Debug)]
pub enum BuildError {
    /// Lowering the inferred IR to C failed.
    Codegen(String),
    /// A filesystem or process-spawn operation failed.
    Io(std::io::Error),
    /// The C compiler ran but exited non-zero.
    Compile { status: ExitStatus, stderr: String },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Codegen(msg) => write!(f, "code generation failed: {}", msg),
            BuildError::Io(err) => write!(f, "io error: {}", err),
            BuildError::Compile { status, stderr } => {
                write!(f, "C compiler failed ({}):\n{}", status, stderr)
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl From<std::io::Error> for BuildError {
    fn from(value: std::io::Error) -> Self {
        BuildError::Io(value)
    }
}

impl From<CodegenError> for BuildError {
    fn from(value: CodegenError) -> Self {
        BuildError::Codegen(value.to_string())
    }
}

impl From<CompilerError> for BuildError {
    fn from(value: CompilerError) -> Self {
        BuildError::Codegen(value.to_string())
    }
}

impl From<TypeError> for BuildError {
    fn from(value: TypeError) -> Self {
        BuildError::Codegen(format!("type error: {:?}", value))
    }
}

/// The result of running a compiled program.
#[derive(Debug)]
pub struct RunReport {
    pub binary: PathBuf,
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Output configuration shared by the build stages.
struct Config {
    name: String,
    dir: PathBuf,
    cc: OsString,
    flags: Vec<OsString>,
}

impl Config {
    fn source_path(&self) -> PathBuf {
        self.dir.join(format!("{}.c", self.name))
    }
    fn binary_path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }
}

/// A configured code-generation-and-build job — the first stage of the outer
/// pipeline: `CBuild::…generate()?.compile()?.run()?`.
///
/// Build one with [`from_body`](Self::from_body) (a single solved IR body),
/// [`from_program`](Self::from_program) (a multi-function [`Program`]), or
/// [`from_builder`](Self::from_builder), tweak `dir`/`cc`/`flags`, then advance
/// through the stages.
pub struct CBuild<'a> {
    config: Config,
    generate: Box<dyn Fn() -> Result<String, BuildError> + 'a>,
}

impl<'a> CBuild<'a> {
    /// Build from a single straight-line IR body that has already been solved.
    pub fn from_body<'s: 'a>(
        name: impl Into<String>,
        body: &'a [Instr],
        registers: &'a RegisterFile,
        solver: &'a Solver<'s>,
    ) -> Self {
        Self::with_generator(name, move || {
            emit_body(body, registers, solver).map_err(BuildError::from)
        })
    }

    /// Build from a whole [`Program`] (solved internally per function).
    pub fn from_program(name: impl Into<String>, program: &'a Program) -> Self {
        Self::with_generator(name, move || emit_code(program).map_err(BuildError::from))
    }

    /// Build from an [`InstructionBuilder`], running the whole inference →
    /// codegen pipeline internally. The counterpart to
    /// [`from_program`](Self::from_program) for a single body.
    pub fn from_builder(
        name: impl Into<String>,
        mut builder: InstructionBuilder,
    ) -> Result<CBuild<'static>, BuildError> {
        let body = builder.body.clone();
        // Generate eagerly while the borrows are live; the closure then just
        // hands back the finished source.
        let source = crate::infer::Inference::new(&body, &builder.register_file)
            .generate_constraints(&mut builder.type_variable_generator)
            .solve(&mut builder.type_variable_generator)?
            .generate_code()?;
        Ok(CBuild::with_generator(name, move || Ok(source.clone())))
    }

    fn with_generator(
        name: impl Into<String>,
        generate: impl Fn() -> Result<String, BuildError> + 'a,
    ) -> Self {
        Self {
            config: Config {
                name: name.into(),
                dir: PathBuf::from("target"),
                cc: OsString::from("cc"),
                flags: vec![OsString::from("-Wall"), OsString::from("-O2")],
            },
            generate: Box::new(generate),
        }
    }

    /// Set the output directory for the `.c` file and binary (default `target`).
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.dir = dir.into();
        self
    }

    /// Set the C compiler to invoke (default `cc`).
    pub fn cc(mut self, cc: impl Into<OsString>) -> Self {
        self.config.cc = cc.into();
        self
    }

    /// Replace the compiler flags (default `-Wall -O2`). `-O1` or higher is
    /// recommended so the allocation/register-pressure optimizations that make
    /// the emitted one-assignment-per-op C efficient actually kick in.
    pub fn flags(mut self, flags: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.config.flags = flags.into_iter().map(Into::into).collect();
        self
    }

    /// Path the C source is (or would be) written to: `<dir>/<name>.c`.
    pub fn source_path(&self) -> PathBuf {
        self.config.source_path()
    }

    /// Path the compiled binary is (or would be) written to: `<dir>/<name>`.
    pub fn binary_path(&self) -> PathBuf {
        self.config.binary_path()
    }

    /// Stage 1: generate the C source (exactly once).
    pub fn generate(self) -> Result<Source, BuildError> {
        let source = (self.generate)()?;
        Ok(Source {
            source,
            config: self.config,
        })
    }
}

/// Stage 2 of the outer pipeline: the generated C source, before it hits disk.
pub struct Source {
    pub source: String,
    config: Config,
}

impl Source {
    pub fn source_path(&self) -> PathBuf {
        self.config.source_path()
    }
    pub fn binary_path(&self) -> PathBuf {
        self.config.binary_path()
    }

    /// Write `<dir>/<name>.c` and compile it to `<dir>/<name>`.
    pub fn compile(self) -> Result<Compiled, BuildError> {
        std::fs::create_dir_all(&self.config.dir)?;
        let c_path = self.config.source_path();
        std::fs::write(&c_path, &self.source)?;

        let binary = self.config.binary_path();
        let output = Command::new(&self.config.cc)
            .args(&self.config.flags)
            .arg(&c_path)
            .arg("-o")
            .arg(&binary)
            .output()?;

        if !output.status.success() {
            return Err(BuildError::Compile {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(Compiled { binary })
    }
}

/// Stage 3 of the outer pipeline: a compiled binary on disk.
pub struct Compiled {
    pub binary: PathBuf,
}

impl Compiled {
    /// Run the binary, capturing its output.
    pub fn run(self) -> Result<RunReport, BuildError> {
        run(&self.binary)
    }
}

/// Run an already-compiled binary and capture its output.
fn run(binary: &Path) -> Result<RunReport, BuildError> {
    let output = Command::new(binary).output()?;
    Ok(RunReport {
        binary: binary.to_path_buf(),
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InstructionBuilder;

    /// Build the classic demo: `sum = (n:=42) + 1`, returned as the result.
    fn demo() -> InstructionBuilder {
        let mut b = InstructionBuilder::default();
        let n = b.const_int(42);
        let one = b.const_int(1);
        let sum = b.binop(crate::instructions::BinOpKind::Add, n, one);
        b.ret(sum);
        b
    }

    #[test]
    fn generate_produces_c_source() {
        let src = CBuild::from_builder("build_test_gen", demo())
            .expect("build")
            .generate()
            .expect("generate");
        assert!(
            src.source.contains("int main(void)"),
            "missing main:\n{}",
            src.source
        );
        assert_eq!(
            src.source_path(),
            std::path::Path::new("target/build_test_gen.c")
        );
    }

    #[test]
    #[ignore = "requires a C compiler; run manually in verification"]
    fn run_reports_output() {
        let report = CBuild::from_builder("build_test_run", demo())
            .expect("build")
            .dir(std::env::temp_dir())
            .generate()
            .expect("generate")
            .compile()
            .expect("compile")
            .run()
            .expect("run");
        // `main` prints the result and exits 0; the value is observed on stdout.
        assert!(report.status.success(), "expected clean exit: {:?}", report);
        assert!(
            report.stdout.contains("result: 43"),
            "expected result 43:\n{}",
            report.stdout
        );
    }
}
