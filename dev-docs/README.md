# WezTerm Developer Documentation

This directory contains technical documentation for developers working on our WezTerm fork codebase. For user-facing documentation from the original WezTerm, see the `./docs` directory.

## Critical Implementation Notes

**IMPORTANT**: Each documentation file below contains a "Critical Implementation Notes" section at the top. You MUST read this section for any area you plan to work on. These notes distinguish between:

- **Architectural requirements**: Fundamental design constraints that cannot be changed without major rewrites
- **Current implementation constraints**: Limitations of the current code that could potentially be fixed
- **Known bugs**: Specific issues in the implementation that need to be avoided

## Documentation Files

### Core Architecture
- **architecture.md** - High-level system architecture, core components, and how they interact
  - *No critical notes* - This is overview documentation

### Rendering System
- **rendering-pipeline.md** - GPU rendering system, Element/Box model, z-index layers, and visual effects
  - **Read Critical Notes if**: Working on rendering, clipping, z-indices, or GPU operations
  - **Key warnings**: Sub-layer constraints, two-phase rendering, failed clipping approaches

### Sidebar Implementation
- **sidebar-patterns.md** - Sidebar implementation patterns, UI components, and lessons learned from specific features
  - **Read Critical Notes if**: Working on sidebars, click detection, animations, or modals
  - **Key warnings**: Window resize bug, thread safety, import patterns, panic-inducing patterns

### Text Rendering
- **text-layout.md** - Text wrapping, font handling, syntax highlighting, and markdown rendering
  - **Read Critical Notes if**: Working on text rendering, fonts, or markdown
  - **Key warnings**: Style color bug, wrap-before-shape requirement, grapheme tracking

## Documentation Standards

### Developer Documentation Structure

Developer documentation is organized into two main categories:

1. **High-level documentation** in `dev-docs/`:
   - Each file contains subject-specific documentation
   - Critical implementation notes at the top of relevant files
   - Detailed explanations and examples below critical notes

2. **Inline documentation** (rustdoc comments):
   - Module-level docs (`//!`) explaining module purpose and key concepts
   - Public API docs (`///`) for structs, traits, functions
   - Implementation notes for complex internal functions

### When to Update Documentation

Update documentation when:
- Modifying architectural patterns or core systems
- Adding new public APIs or changing existing ones
- Implementing new UI patterns or components
- Solving complex problems that others might encounter
- Discovering non-obvious constraints or behaviors

### Rustdoc Guidelines

Add rustdoc comments to:
- **Modules**: Use `//!` at the top to explain the module's purpose, main components, and how it fits into the larger system
- **Public APIs**: Use `///` above all public structs, traits, enums, and functions
- **Complex internals**: Document non-obvious implementation details, especially around rendering, text layout, or event handling
- **Cross-references**: Link to related modules/functions using backticks (e.g., `see `wrap_styled_text()` for details`)

Example:
```rust
//! Module-level documentation explaining purpose

/// Struct documentation explaining what it represents and how to use it
/// 
/// Include examples if the usage isn't obvious.
pub struct Example {
    /// Document public fields if their purpose isn't clear from the name
    pub field: Type,
}
```

### How to Document

- Reference specific files and line numbers when possible (e.g., `termwindow/box_model.rs:1234`)
- Prefer linking to code over duplicating it
- Document the "why" not just the "what"
- Include lessons learned and gotchas
- Keep examples minimal but complete

### Maintaining Accuracy

- When changing code, update related documentation in both `dev-docs/` and rustdoc comments
- Reference specific files and line numbers when possible (e.g., `box_model.rs:1234`)
- Run occasional documentation reviews to ensure accuracy
- Keep the architecture diagrams and z-index assignments in sync with code

### Documentation Review Process

Before committing significant changes:
1. Update relevant `dev-docs/` files if architecture or patterns changed
2. Add/update rustdoc comments for modified public APIs
3. Ensure examples still work if provided
4. Verify cross-references are still valid