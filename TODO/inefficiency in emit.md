You mentioned before

"
There's a remaining inefficiency in emit_code I left alone: it solves each function twice (once in the signature pass, once in the body pass), because all signatures must exist before any body can resolve inter-function calls. Fixing it means keeping the solved state across the two passes, which tangles lifetimes (a CodeGen borrows a Solver that borrows a per-function tvg). Happy to tackle it if you want.
"

can you please analyze this and propose a plan.
