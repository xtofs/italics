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
use crate::codegen::{CodegenError, ProgramCodegenError, emit_c, emit_code};
use crate::instructions::Instr;
use crate::program::IRProgram;
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

impl From<ProgramCodegenError> for BuildError {
    fn from(value: ProgramCodegenError) -> Self {
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

/// A configured code-generation-and-build job.
///
/// Build one with [`CBuild::from_body`] (a single solved IR body) or
/// [`CBuild::from_program`] (a multi-function [`IRProgram`]), tweak the output
/// directory / compiler / flags if needed, then call one of the three terminal
/// methods.
pub struct CBuild<'a> {
    name: String,
    dir: PathBuf,
    cc: OsString,
    flags: Vec<OsString>,
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
            emit_c(body, registers, solver).map_err(BuildError::from)
        })
    }

    /// Build from a whole [`IRProgram`] (solved internally per function).
    pub fn from_program(name: impl Into<String>, program: &'a IRProgram) -> Self {
        Self::with_generator(name, move || emit_code(program).map_err(BuildError::from))
    }

    /// Build from an [`InstructionBuilder`], running the whole constraint-generation,
    /// solve, and lowering pipeline internally. This is the straight-line
    /// counterpart to [`from_program`](Self::from_program) for callers that just
    /// want C out of an IR body without driving the solver themselves.
    pub fn from_builder(
        name: impl Into<String>,
        mut builder: InstructionBuilder,
    ) -> Result<CBuild<'static>, BuildError> {
        let body = builder.body.clone();
        let mut solver = Solver::new(&mut builder.type_variable_generator);
        let mut constraints = Vec::new();
        for instr in &body {
            constraints.extend(solver.generate_constraints(instr));
        }
        solver.solve(&constraints)?;
        // Generate eagerly while the solver is in scope; the harness then just
        // hands back the finished source.
        let source = emit_c(&body, &builder.register_file, &solver)?;
        Ok(CBuild::with_generator(name, move || Ok(source.clone())))
    }

    fn with_generator(
        name: impl Into<String>,
        generate: impl Fn() -> Result<String, BuildError> + 'a,
    ) -> Self {
        Self {
            name: name.into(),
            dir: PathBuf::from("target"),
            cc: OsString::from("cc"),
            flags: vec![OsString::from("-Wall"), OsString::from("-O2")],
            generate: Box::new(generate),
        }
    }

    /// Set the output directory for the `.c` file and binary (default `target`).
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = dir.into();
        self
    }

    /// Set the C compiler to invoke (default `cc`).
    pub fn cc(mut self, cc: impl Into<OsString>) -> Self {
        self.cc = cc.into();
        self
    }

    /// Replace the compiler flags (default `-Wall -O2`). `-O1` or higher is
    /// recommended so the allocation/register-pressure optimizations that make
    /// the emitted one-assignment-per-op C efficient actually kick in.
    pub fn flags(mut self, flags: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.flags = flags.into_iter().map(Into::into).collect();
        self
    }

    /// Path the C source is (or would be) written to: `<dir>/<name>.c`.
    pub fn source_path(&self) -> PathBuf {
        self.dir.join(format!("{}.c", self.name))
    }

    /// Path the compiled binary is (or would be) written to: `<dir>/<name>`.
    pub fn binary_path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    /// Generate the C source.
    pub fn generate(&self) -> Result<String, BuildError> {
        (self.generate)()
    }

    /// Generate the C source, write it to [`c_path`](Self::c_path), and compile
    /// it to [`binary_path`](Self::binary_path). Returns the binary path.
    pub fn compile(&self) -> Result<PathBuf, BuildError> {
        let source = self.generate()?;

        // write to file
        std::fs::create_dir_all(&self.dir)?;

        let c_path = self.source_path();
        std::fs::write(&c_path, source)?;

        let binary = self.binary_path();
        let output = Command::new(&self.cc)
            .args(&self.flags)
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
        Ok(binary)
    }

    /// Generate, compile, then run the program, capturing its output.
    pub fn run(&self) -> Result<RunReport, BuildError> {
        let binary = self.compile()?;
        run(&binary)
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
    use crate::solver::Solver;

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
        let mut b = demo();
        let body = b.body.clone();
        let mut solver = Solver::new(&mut b.type_variable_generator);
        let cs: Vec<_> = body
            .iter()
            .flat_map(|i| solver.generate_constraints(i))
            .collect();
        solver.solve(&cs).unwrap();

        let build = CBuild::from_body("build_test_gen", &body, &b.register_file, &solver);
        let c = build.generate().expect("generate");
        assert!(c.contains("int main(void)"), "missing main:\n{}", c);
        assert_eq!(
            build.source_path(),
            std::path::Path::new("target/build_test_gen.c")
        );
    }

    #[test]
    #[ignore = "requires a C compiler; run manually in verification"]
    fn run_reports_output() {
        let mut b = demo();
        let body = b.body.clone();
        let mut solver = Solver::new(&mut b.type_variable_generator);
        let cs: Vec<_> = body
            .iter()
            .flat_map(|i| solver.generate_constraints(i))
            .collect();
        solver.solve(&cs).unwrap();

        let report = CBuild::from_body("build_test_run", &body, &b.register_file, &solver)
            .dir(std::env::temp_dir())
            .run()
            .expect("generate/compile/run");
        // `main` prints the result and exits 0; the value is observed on stdout.
        assert!(report.status.success(), "expected clean exit: {:?}", report);
        assert!(
            report.stdout.contains("result: 43"),
            "expected result 43:\n{}",
            report.stdout
        );
    }
}
