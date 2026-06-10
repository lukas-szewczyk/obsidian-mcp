# List available recipes
default:
    @just --list

# Run MCP Inspector against the server with the test vault
inspector:
    npx @modelcontextprotocol/inspector \
        -e OBSIDIAN_VAULT_PATH="/Users/lukasz/Desktop/test-vault" \
        -e OBSIDIAN_VAULT_NAME="test-vault" \
        -e OBSIDIAN_CLI="/Applications/Obsidian.app/Contents/MacOS/obsidian" \
        cargo run
