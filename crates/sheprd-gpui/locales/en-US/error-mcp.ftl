# `sheprd mcp`: a tool's arguments refused, or the server unable to serve.

# $name is an argument's name; $choices the values it takes, comma-separated.
error-mcp-choice = { $name } must be one of: { $choices }
error-mcp-executable = Could not tell where this sheprd executable is
error-mcp-missing = { $name } is required
error-mcp-off = The MCP server is off; turn on MCP server in Sheprd's Settings > General
error-mcp-range = { $name } must be from { $min } to { $max }
# $error is the server's own text.
error-mcp-serve = Could not serve MCP: { $error }
error-mcp-serve-only = `sheprd mcp` serves only when an MCP client starts it
error-mcp-too-long = { $name } must be at most { $max } bytes
# $expected says what kind of value, such as "a string".
error-mcp-type = { $name } must be { $expected }
error-mcp-unknown = This tool takes no argument named { $name }
