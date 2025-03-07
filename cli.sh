#!/bin/bash

# cli.sh - Helper script to build and run the CLI application

set -e  # Exit on any error

# Default values
BUILD_DIR="build"
SKIP_BUILD=false
VERBOSE=false

# Parse command-line arguments
PARAMS=""
while (( "$#" )); do
  case "$1" in
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    -v|--verbose)
      VERBOSE=true
      shift
      ;;
    --) # End of options
      shift
      break
      ;;
    -*) # Unsupported flags
      echo "Error: Unsupported flag $1" >&2
      exit 1
      ;;
    *) # Preserve positional arguments
      PARAMS="$PARAMS $1"
      shift
      ;;
  esac
done
# Set positional arguments in their proper place
eval set -- "$PARAMS"

# Function to log messages when verbose is enabled
log() {
  if [ "$VERBOSE" = true ]; then
    echo "[cli.sh] $1"
  fi
}

# Make sure we're in the project root directory
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_ROOT"

log "Project root: $PROJECT_ROOT"

# Create build directory if it doesn't exist
if [ ! -d "$BUILD_DIR" ]; then
  log "Creating build directory: $BUILD_DIR"
  mkdir -p "$BUILD_DIR"
fi

# Build the project (unless skipped)
if [ "$SKIP_BUILD" = false ]; then
  log "Building project..."
  
  # Check if we need to configure first
  if [ ! -f "$BUILD_DIR/build.ninja" ]; then
    log "Configuring with CMake..."
    cd "$BUILD_DIR"
    cmake -G Ninja ..
    cd "$PROJECT_ROOT"
  fi
  
  # Build with Ninja
  log "Running Ninja build..."
  cd "$BUILD_DIR"
  ninja
  cd "$PROJECT_ROOT"
  
  log "Build completed successfully"
fi

# Run the CLI application with any provided arguments
log "Running CLI application..."
"$BUILD_DIR/cli_app" "$@"

# Exit with the same status as the CLI application
exit $?