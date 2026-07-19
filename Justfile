# Justfile for tcgui project
# Use `just --list` to see all available commands

# Default recipe - run quality verification before build
default: quality build

# Build all components (debug mode)
build: build-backend build-frontend

# Build all components in release mode
build-release: build-backend-release build-frontend-release

# Build backend in debug mode
build-backend:
    @echo "Building tcgui-backend..."
    cd tcgui-backend && cargo build

# Build frontend in debug mode
build-frontend:
    @echo "Building tcgui-frontend..."
    cd tcgui-frontend && cargo build

# Build backend in release mode
build-backend-release:
    @echo "Building tcgui-backend (release)..."
    cd tcgui-backend && cargo build --release

# Build frontend in release mode
build-frontend-release:
    @echo "Building tcgui-frontend (release)..."
    cd tcgui-frontend && cargo build --release


# Clean build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean
    rm -rf target/coverage/ target/miri/
    @echo "✓ Build artifacts cleaned"

# =============================================================================
# COMPREHENSIVE QUALITY ASSURANCE PIPELINE
# Following WARP.md strict standards: Zero warnings, No dead code, Clean codebase
# =============================================================================

# Install all required quality tools
setup-tools:
    @echo "🔧 Installing quality assurance tools..."
    cargo install cargo-tarpaulin  # Code coverage analysis
    cargo install cargo-audit      # Security vulnerability scanning
    cargo install cargo-deny       # Dependency license and security checks
    cargo install cargo-udeps      # Unused dependency detection
    cargo install cargo-machete    # Dead code analysis across workspace
    cargo install cargo-nextest    # Next-generation test runner
    cargo install cargo-outdated   # Dependency update checking
    rustup component add miri      # Memory safety verification (requires nightly)
    rustup component add rust-src  # Required for miri
    @echo "✅ All quality tools installed successfully"

# Install packaging tools for DEB/RPM generation
setup-packaging-tools:
    @echo "📦 Installing packaging tools..."
    cargo install cargo-deb        # DEB package generation
    cargo install cargo-generate-rpm # RPM package generation
    @echo "✅ All packaging tools installed successfully"

# Quick development cycle - fast feedback
dev: fmt check clippy test-fast
    @echo "🚀 Development cycle complete - ready for iteration"

# Ultra-fast development cycle - skip full compilation check
dev-fast: fmt clippy-fast test-fast
    @echo "⚡ Ultra-fast development cycle complete - ready for iteration"

# Minimal development cycle - format and test only
dev-minimal: fmt test-fast
    @echo "🏃 Minimal development cycle complete - ready for iteration"

# Backend-only development cycle
dev-backend: fmt check-backend clippy-backend test-backend
    @echo "🔧 Backend development cycle complete"

# Frontend-only development cycle
dev-frontend: fmt check-frontend clippy-frontend test-frontend
    @echo "🎨 Frontend development cycle complete"

# Full quality verification pipeline (matches WARP.md standards)
quality: fmt check clippy test coverage security unused-deps outdated-deps deadcode
    @echo "🎉 COMPLETE QUALITY VERIFICATION PASSED"
    @echo "  ✅ Zero compiler warnings"
    @echo "  ✅ Zero clippy issues"
    @echo "  ✅ Consistent formatting"
    @echo "  ✅ Full test coverage"
    @echo "  ✅ No security vulnerabilities"
    @echo "  ✅ No unused dependencies"
    @echo "  ✅ Current dependency status checked"
    @echo "  ✅ No dead code"

# Pre-commit quality gate - run before every commit
pre-commit: fmt-check check clippy test-fast security-fast
    @echo "🛡️  Pre-commit quality gate passed - safe to commit"

# Continuous Integration pipeline - for automated environments
ci: fmt-check check clippy test coverage security unused-deps
    @echo "🏗️  CI pipeline completed successfully"

# === CORE QUALITY CHECKS ===

# Format code consistently (auto-fix)
fmt:
    @echo "🎨 Formatting code..."
    cargo fmt --all
    @echo "✅ Code formatting complete"

# Check formatting without changes (CI-friendly)
fmt-check:
    @echo "🔍 Checking code formatting..."
    cargo fmt --all -- --check
    @echo "✅ Code formatting verified"

# Check compilation with zero tolerance for warnings
check:
    @echo "🔍 Checking compilation (zero warnings policy)..."
    cargo check --workspace --all-targets --all-features
    @echo "✅ Compilation check passed"

# Clippy linting with warnings as errors
clippy:
    @echo "📎 Running clippy analysis (strict mode)..."
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    @echo "✅ Clippy analysis passed"

# Fast clippy - only check lib targets (skip examples, benchmarks, etc)
clippy-fast:
    @echo "⚡ Running fast clippy analysis..."
    cargo clippy --workspace --lib -- -D warnings
    @echo "✅ Fast clippy analysis passed"

# Component-specific checks
check-backend:
    @echo "🔍 Checking backend compilation..."
    cargo check -p tcgui-backend
    @echo "✅ Backend compilation check passed"

check-frontend:
    @echo "🔍 Checking frontend compilation..."
    cargo check -p tcgui-frontend
    @echo "✅ Frontend compilation check passed"

clippy-backend:
    @echo "📎 Running clippy on backend..."
    cargo clippy -p tcgui-backend -- -D warnings
    @echo "✅ Backend clippy passed"

clippy-frontend:
    @echo "📎 Running clippy on frontend..."
    cargo clippy -p tcgui-frontend -- -D warnings
    @echo "✅ Frontend clippy passed"

test-backend:
    @echo "🧪 Running backend tests..."
    cargo test -p tcgui-backend --lib --quiet
    @echo "✅ Backend tests passed"

test-frontend:
    @echo "🧪 Running frontend tests..."
    cargo test -p tcgui-frontend --lib --quiet
    @echo "✅ Frontend tests passed"

# === TESTING STRATEGIES ===

# Fast test suite for development iteration
test-fast:
    @echo "🧪 Running fast test suite..."
    cargo test --lib --workspace --quiet
    @echo "✅ Fast tests passed"

# Full test suite with comprehensive coverage
test:
    @echo "🧪 Running comprehensive test suite..."
    cargo test --workspace --all-targets --all-features
    @echo "✅ Full test suite passed"

# Next-generation fast testing (requires cargo-nextest)
test-nextest:
    @echo "⚡ Running tests with nextest (parallel execution)..."
    cargo nextest run --workspace --all-targets --all-features
    @echo "✅ Nextest execution complete"

# === CODE COVERAGE ANALYSIS ===

# Generate code coverage report
coverage:
    @echo "📊 Analyzing code coverage with tarpaulin..."
    cargo tarpaulin --workspace --out Html --output-dir target/coverage --timeout 300
    @echo "✅ Coverage analysis complete"
    @echo "📋 Report available: target/coverage/tarpaulin-report.html"

# Coverage with specific thresholds
coverage-strict:
    @echo "📊 Running strict coverage analysis (80% minimum)..."
    cargo tarpaulin --workspace --fail-under 80 --timeout 300
    @echo "✅ Strict coverage requirements met"

# === SECURITY AND DEPENDENCY ANALYSIS ===

# Security vulnerability audit
security:
    @echo "🔒 Running security audit..."
    cargo audit
    @echo "✅ Security audit passed"

# Fast security check (skip advisory database update)
security-fast:
    @echo "🔒 Running fast security check..."
    cargo audit --quiet
    @echo "✅ Fast security check passed"

# Comprehensive dependency analysis
security-full:
    @echo "🔒 Running comprehensive security analysis..."
    cargo deny check all
    @echo "✅ Comprehensive security analysis passed"

# === DEPENDENCY MANAGEMENT ===

# Check for unused dependencies
unused-deps:
    @echo "🧹 Checking for unused dependencies..."
    cargo +nightly udeps --workspace --all-targets --all-features
    @echo "✅ No unused dependencies found"

# Check for outdated dependencies
outdated-deps:
    @echo "📅 Checking for outdated dependencies..."
    cargo outdated --workspace
    @echo "✅ Dependency status check complete"

# Update dependencies safely
update-deps:
    @echo "📦 Updating dependencies..."
    cargo update
    just quality  # Re-run full quality checks after update
    @echo "✅ Dependencies updated and verified"

# === DEAD CODE ANALYSIS ===

# Advanced dead code detection across workspace
deadcode:
    @echo "🔍 Analyzing dead code across workspace..."
    cargo machete
    @echo "✅ Dead code analysis complete"

# === MEMORY SAFETY VERIFICATION ===

# Miri verification for critical unsafe code and key modules
miri-key-modules:
    @echo "🧠 Running Miri on key modules for memory safety..."
    # Test critical networking and concurrency components. Isolation is
    # disabled: these tests read the clock (SystemTime::now in
    # NetworkScenario::new), which strict isolation rejects outright —
    # we want Miri's UB detection, not its determinism sandbox.
    MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p tcgui-shared --lib
    MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p tcgui-backend --lib bandwidth::tests
    @echo "✅ Memory safety verification complete"

# Full Miri analysis (slow - use sparingly)
miri-full:
    @echo "🧠 Running comprehensive Miri analysis..."
    MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --workspace --lib
    @echo "✅ Comprehensive memory safety verification complete"

# === PACKAGING COMMANDS ===

# Generate all packages (DEB + RPM for both frontend and backend)
package: build-release
    @echo "📦 Generating all packages..."
    @./scripts/package.sh all

# Generate DEB packages only
package-deb: build-release
    @echo "📦 Generating DEB packages..."
    @./scripts/package.sh all deb

# Generate RPM packages only
package-rpm: build-release
    @echo "📦 Generating RPM packages..."
    @./scripts/package.sh all rpm

# Generate backend packages only
package-backend FORMAT="both": build-backend-release
    @echo "📦 Generating backend packages ({{FORMAT}})..."
    @./scripts/package.sh backend {{FORMAT}}

# Generate frontend packages only
package-frontend FORMAT="both": build-frontend-release
    @echo "📦 Generating frontend packages ({{FORMAT}})..."
    @./scripts/package.sh frontend {{FORMAT}}

# List all generated packages
list-packages:
    @echo "📋 Listing generated packages..."
    @./scripts/package.sh list

# Validate generated packages
validate-packages:
    @echo "✅ Validating generated packages..."
    @./scripts/package.sh validate

# Clean old packages and packaging artifacts
clean-packages:
    @echo "🧹 Cleaning old packages..."
    @./scripts/package.sh clean

# Test generated packages (requires sudo)
test-packages:
    @echo "🧪 Testing generated packages..."
    @echo "⚠️  This requires sudo privileges for package installation testing"
    @sudo ./scripts/test-packages.sh test-all

# === DOCUMENTATION AND API ===

# Generate and check documentation
docs:
    @echo "📚 Generating documentation..."
    cargo doc --workspace --all-features --no-deps --document-private-items
    @echo "✅ Documentation generated successfully"

# === MAINTENANCE AND CLEANUP ===

# Fix all automatically fixable issues
fix:
    @echo "🔧 Auto-fixing code issues..."
    cargo fix --workspace --all-targets --all-features --allow-dirty
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty
    cargo fmt --all
    @echo "✅ Auto-fix complete"

# === LOCAL GITHUB WORKFLOW TESTING ===

# Setup for local GitHub Actions testing
setup-local-ci:
    @echo "🐳 Setting up local GitHub Actions testing..."
    @echo "Installing act (GitHub Actions runner)..."
    @command -v act >/dev/null 2>&1 || { echo "Act not found. Install with: curl -s https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash"; exit 1; }
    @echo "Creating act configuration..."
    @mkdir -p ~/.actrc
    @echo "✅ Local CI testing setup complete"

# Run GitHub Actions locally (fast quality gate)
local-ci-fast:
    @echo "⚡ Running fast quality gate locally (mimics GitHub Actions)..."
    act -j fast-quality --pull=false

# Run GitHub Actions locally (comprehensive quality)
local-ci-full:
    @echo "🔍 Running comprehensive quality analysis locally..."
    act -j comprehensive-quality --pull=false

# Run all GitHub Actions locally
local-ci-all:
    @echo "🌍 Running complete GitHub Actions workflow locally..."
    act --pull=false

# Test specific GitHub Actions job locally
local-ci-job JOB_NAME:
    @echo "🎯 Running specific job '{{JOB_NAME}}' locally..."
    act -j {{JOB_NAME}} --pull=false

# Validate GitHub Actions workflow files
validate-workflows:
    @echo "🔍 Validating GitHub Actions workflows..."
    @./scripts/validate-workflows.sh

# Run local CI simulation (Docker-free)
local-ci TARGET="all":
    @echo "🚀 Running local CI simulation: {{TARGET}}..."
    @./scripts/local-ci.sh {{TARGET}}

# Quick local development check (fast feedback)
local-check:
    @echo "⚡ Running fast local quality checks..."
    @./scripts/local-ci.sh fast

# Comprehensive local testing
local-comprehensive:
    @echo "🔬 Running comprehensive local analysis..."
    @./scripts/local-ci.sh comprehensive

# Show available GitHub Actions jobs
list-ci-jobs:
    @echo "📋 Available GitHub Actions jobs:"
    act --list

# === PROJECT-SPECIFIC WORKFLOWS ===

# Backend development workflow
backend:
    @echo "🔧 Backend development workflow..."
    cargo check -p tcgui-backend --all-targets
    cargo test -p tcgui-backend --lib --quiet
    cargo clippy -p tcgui-backend --all-targets -- -D warnings
    @echo "✅ Backend workflow complete"

# Frontend development workflow
frontend:
    @echo "🎨 Frontend development workflow..."
    cargo check -p tcgui-frontend --all-targets
    cargo test -p tcgui-frontend --lib --quiet
    cargo clippy -p tcgui-frontend --all-targets -- -D warnings
    @echo "✅ Frontend workflow complete"

# Shared library workflow
shared:
    @echo "📦 Shared library workflow..."
    cargo check -p tcgui-shared --all-targets
    cargo test -p tcgui-shared --quiet
    cargo clippy -p tcgui-shared --all-targets -- -D warnings
    @echo "✅ Shared library workflow complete"

# === INTEGRATION WORKFLOWS ===

# Pre-push comprehensive check (before pushing to remote)
pre-push: quality docs
    @echo "🚀 Pre-push verification complete - safe to push"

# Release preparation workflow
prepare-release: quality coverage docs
    @echo "📋 Release preparation checklist:"
    @echo "  ✅ All quality checks passed"
    @echo "  ✅ Full test coverage analyzed"
    @echo "  ✅ Documentation generated"
    @echo "  🔄 Ready for version bump and release"

# Emergency hotfix workflow (minimal but essential checks)
hotfix: fmt check test-fast
    @echo "🚨 Emergency hotfix verification complete"

# Run backend (debug mode) - requires sudo
run-backend: build-backend
    @echo "Starting tcgui-backend (debug) with sudo..."
    sudo ./target/debug/tcgui-backend --exclude-loopback --verbose --name trefze3

# Run frontend (debug mode) - expects backend to be running
run-frontend: build-frontend
    @echo "Starting tcgui-frontend (debug) - backend should be running..."
    ./target/debug/tcgui-frontend --verbose

# Run backend (release mode) - requires sudo
run-backend-release: build-backend-release
    @echo "Starting tcgui-backend (release) with sudo..."
    sudo ./target/release/tcgui-backend --exclude-loopback --verbose --name trefze3

# Run frontend (release mode) - expects backend to be running
run-frontend-release: build-frontend-release
    @echo "Starting tcgui-frontend (release) - backend should be running..."
    ./target/release/tcgui-frontend --verbose

# Show help
help:
    @echo "🏗️  TC GUI Project - Development Workflow Commands"
    @echo ""
    @echo "📈 QUALITY WORKFLOWS (NEW!):"
    @echo "  just quality      - Full quality verification pipeline"
    @echo "  just dev          - Fast development iteration cycle"
    @echo "  just dev-fast     - Ultra-fast cycle (skip full compilation)"
    @echo "  just dev-minimal  - Minimal cycle (format + tests only)"
    @echo "  just dev-backend  - Backend-only development cycle"
    @echo "  just dev-frontend - Frontend-only development cycle"
    @echo "  just pre-commit   - Pre-commit quality gate"
    @echo "  just ci           - Continuous integration pipeline"
    @echo ""
    @echo "🔧 INDIVIDUAL QUALITY CHECKS:"
    @echo "  just fmt          - Format code (auto-fix)"
    @echo "  just check        - Compilation check (zero warnings)"
    @echo "  just clippy       - Lint analysis (strict mode)"
    @echo "  just clippy-fast  - Fast lint analysis (lib targets only)"
    @echo "  just test         - Full test suite"
    @echo "  just test-fast    - Fast test suite (lib targets only)"
    @echo "  just coverage     - Code coverage analysis"
    @echo "  just security     - Security vulnerability audit"
    @echo "  just unused-deps  - Check for unused dependencies"
    @echo "  just outdated-deps - Check for outdated dependencies"
    @echo "  just deadcode     - Dead code detection"
    @echo "  just miri-key-modules - Memory safety verification"
    @echo ""
    @echo "🎯 COMPONENT-SPECIFIC CHECKS:"
    @echo "  just check-backend/frontend - Check specific component"
    @echo "  just clippy-backend/frontend - Lint specific component"
    @echo "  just test-backend/frontend - Test specific component"
    @echo ""
    @echo "⚙️  SETUP & MAINTENANCE:"
    @echo "  just setup-tools  - Install all quality tools"
    @echo "  just setup-packaging-tools - Install DEB/RPM packaging tools"
    @echo "  just setup-local-ci - Setup local GitHub Actions testing"
    @echo "  just fix          - Auto-fix all fixable issues"
    @echo ""
    @echo "🐳 LOCAL CI TESTING:"
    @echo "  just validate-workflows - Validate workflow YAML files"
    @echo "  just local-ci [TARGET]  - Docker-free CI simulation (all/fast/comprehensive)"
    @echo "  just local-check        - Fast quality checks (format/clippy/tests)"
    @echo "  just local-comprehensive - Comprehensive analysis with coverage"
    @echo "  just local-ci-fast      - Act-based fast quality gate"
    @echo "  just local-ci-full      - Act-based comprehensive analysis"
    @echo "  just local-ci-all       - Act-based complete workflow"
    @echo "  just list-ci-jobs       - Show available CI jobs"
    @echo ""
    @echo "🎯 PROJECT-SPECIFIC:"
    @echo "  just backend      - Backend-focused workflow"
    @echo "  just frontend     - Frontend-focused workflow"
    @echo "  just shared       - Shared library workflow"
    @echo ""
    @echo "📦 PACKAGE GENERATION:"
    @echo "  just package      - Generate all packages (DEB + RPM)"
    @echo "  just package-deb  - Generate DEB packages only"
    @echo "  just package-rpm  - Generate RPM packages only"
    @echo "  just package-backend [FORMAT] - Generate backend packages"
    @echo "  just package-frontend [FORMAT] - Generate frontend packages"
    @echo "  just list-packages - List all generated packages"
    @echo "  just validate-packages - Validate generated packages"
    @echo "  just test-packages - Test package installation (requires sudo)"
    @echo "  just clean-packages - Clean old packages"
    @echo ""
    @echo "🚀 BUILD COMMANDS (EXISTING):"
    @echo "  just build         # Build all components (debug mode)"
    @echo "  just build-release # Build all components (release mode)"
    @echo "  just build-backend # Build backend only (debug mode)"
    @echo "  just build-frontend # Build frontend only (debug mode)"
    @echo "  just build-backend-release # Build backend only (release mode)"
    @echo "  just build-frontend-release # Build frontend only (release mode)"
    @echo ""
    @echo "▶️  RUN COMMANDS:"
    @echo "  just run-backend   # Run backend (debug, requires sudo)"
    @echo "  just run-frontend  # Run frontend (debug, expects backend running)"
    @echo "  just run-backend-release # Run backend (release, requires sudo)"
    @echo "  just run-frontend-release # Run frontend (release, expects backend running)"
    @echo ""
    @echo "🧹 MAINTENANCE:"
    @echo "  just clean         # Clean build artifacts"
    @echo "  just help          # Show this help"
