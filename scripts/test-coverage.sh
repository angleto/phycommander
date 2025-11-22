#!/bin/bash
# Test coverage script using tarpaulin

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${GREEN}Installing cargo-tarpaulin...${NC}"
cargo install cargo-tarpaulin

echo -e "${GREEN}Running tests with coverage...${NC}"
cd "$PROJECT_ROOT/physerver"

cargo tarpaulin \
    --out Html \
    --out Xml \
    --output-dir coverage \
    --exclude-files "tests/*" \
    --exclude-files "benches/*" \
    --timeout 300 \
    --verbose

echo -e "${GREEN}Coverage report generated in physerver/coverage/${NC}"
echo "Open physerver/coverage/index.html in your browser"
