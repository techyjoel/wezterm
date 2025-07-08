# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Refer to ../backend/SPEC.md for a detailed spec of this project and use TASKS.md to track the project and tasks.

**Key Guidance**
- Think critically and be skeptical of prior work. 
- Think like a lead software engineer and architect. 
- Fix problems you find that will break things (but don't do needless work). 
- Consider multiple concepts for how to solve problems before you write code, and pick the best one that aligns with the codebase.
- Before editing things, use a subagent to examine the current codebase to ensure you fully understand all relevant portions (but subagents should not modify code, you must tell them not to). 
- Do not make guesses, ensure you understand!
- Do not change architecture, approach, or make other major decisions without the user agreeing first

## Build and Development Commands

### Initial Setup
```bash
# Install system dependencies
./get-deps

# Check Rust version (minimum 1.71.0)
./ci/check-rust-version.sh
```

### Common Development Commands
```bash
# Type-check without building, only show errors (use this form unless requested to fix warnings by the user)
cargo check 2>&1 | awk '/^error/ {print; in_block=1; next} in_block { if (/^$/) in_block=0; else print }'

# Type-check without building (fastest iteration)
cargo check

# Build in debug mode
cargo build

# Build in release mode
cargo build --release 2>&1 | tail -50

# Run in debug mode
cargo run

# Run with backtrace for debugging
RUST_BACKTRACE=1 cargo run

# Run all tests
cargo test --all

# Auto-format code (required before PR submission)
cargo +nightly fmt --all

# Build and serve documentation locally
ci/build-docs.sh serve

# Debug with gdb
cargo build
gdb ./target/debug/wezterm
# In gdb: break rust_panic, run, bt

# Run the built binary with debug logging
WEZTERM_LOG=debug ./target/release/wezterm
```

### Development process
The workflow you must use is:
1. Understand the desired change thoroughly. Ask the user questions if you're not sure about any aspects
2. Think carefully about how to properly implement the change within the codebase
3. Create an outline of the proposed work for the user to review, or use an existing outline in TASKS.md if it exists
4. Create a branch if doing any notable work and not already in a branch (feature or other material change)
5. Once in agreement on the proposed work, implement the change
6. Run auto-format of code
7. You MUST run a type check and then a release mode build to test if changes compile successfully before proceeding
9. If you made any notable changes, and especially GUI changes, then prompt the user to run the build to test it (since you won't be able to see the graphic results). DO NOT move on to further steps until this step is complete.
10. Git add and commit (only after succesfully compiling and testing). Then if on a branch, git push (if not then the user will push when desired)
11. Update TASKS.md (if it's in use)
  - Check off tasks that are fully complete
  - Note partiallly-completed work that's done (and what's left to do), but do not check off partially-completed tasks
  - Correct the task items to reflect the as-built conditions (i.e. re-word things or add things so the task list reflects the new codebase)
  - Add implementation details that have been built which will need to be referenced for future tasks

### Git Commit Requirements
   - Include "Created with AI assistance" in your git commit messages.
   - DO NOT say anything else about AI like "co-authored" or anything else. 
   - DO NOT mention Claude.
   - Don't use any emojii in git commit messages

## Architecture and Development

For detailed technical documentation, see the `dev-docs/` directory. 

**YOU MUST READ `dev-docs/README.md` EVERY SESSION** - it explains the documentation structure and which Critical Implementation Notes sections you need to read based on what you're working on.

Documentation files:
- **architecture.md** - System architecture, core components, workspace structure
- **rendering-pipeline.md** - GPU rendering system, Element model, z-index layers
- **sidebar-patterns.md** - Sidebar implementation patterns and UI components
- **text-layout.md** - Text wrapping, font handling, and markdown rendering

Our custom config file is located at `./clibuddy/wezterm.lua`

## Implementation Plans

All implementation plans and task breakdowns have been moved to TASKS.md for centralized tracking and management. Please refer to TASKS.md for:

- Detailed project phases and timelines
- Task breakdowns with dependencies
- Library recommendations
- Integration points and patterns
- Testing strategies

