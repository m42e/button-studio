# Button Studio development

- Keep GUI and headless MCP modes in the single `button-studio` executable.
- Keep both modes on the shared document, `model`, and `render` modules so GUI edits, MCP edits, project files, and exports stay consistent.
- The MCP server uses the official Rust SDK: https://github.com/modelcontextprotocol/rust-sdk
- Protocol documentation is available at https://modelcontextprotocol.io/docs and https://modelcontextprotocol.io/specification
- MCP tool parameters must derive `serde::Deserialize` and `rmcp::schemars::JsonSchema`.
- The server uses stdio transport. Never write diagnostics or other non-protocol content to stdout.