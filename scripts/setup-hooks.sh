#!/bin/bash

# Setup script for git hooks
# Run this after cloning the repository

echo "Setting up git hooks..."

# Configure git to use the .githooks directory
git config core.hooksPath .githooks

# Make hooks executable
chmod +x .githooks/*

echo "Git hooks configured successfully."
echo ""
echo "Commit message format will be validated using Conventional Commits."
echo "See CONTRIBUTING.md for details."
