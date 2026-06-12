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

# Run integration tests against the live Obsidian test vault
integration:
    OBSIDIAN_VAULT_NAME="test-vault" \
    OBSIDIAN_VAULT_PATH="{{justfile_directory()}}/test-vault" \
    OBSIDIAN_CLI="/Applications/Obsidian.app/Contents/MacOS/obsidian" \
    cargo test --test resources -- --test-threads=1
