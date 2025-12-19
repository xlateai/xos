#!/bin/bash
# Helper script to run pod install with proper encoding for colored output

# Set UTF-8 encoding (required for CocoaPods colored output)
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# Run pod install
pod install "$@"


