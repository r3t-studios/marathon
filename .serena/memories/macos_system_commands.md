# macOS (Darwin) System Commands

This document covers macOS-specific system commands and utilities that may differ from standard Unix/Linux systems.

## File System Operations

### Finding Files
```bash
# Standard Unix find (works on macOS)
find . -name "*.rs"
find . -type f -name "Cargo.toml"

# macOS Spotlight search (faster for indexed content)
mdfind -name "rustfmt.toml"
mdfind "kind:rust-source"

# Locate database (if enabled)
locate pattern
```

### Listing & Viewing
```bash
# List with details
ls -la
ls -lh  # human-readable sizes
ls -lhS # sorted by size
ls -lht # sorted by modification time

# View file contents
cat file.txt
head -20 file.txt
tail -50 file.txt
less file.txt  # paginated view

# Quick Look (macOS-specific)
qlmanage -p file.txt  # preview file
```

### Directory Navigation
```bash
cd /path/to/directory
cd ~              # home directory
cd -              # previous directory
pwd               # print working directory
pushd /path       # push to directory stack
popd              # pop from directory stack
```

## Text Processing

### Searching in Files
```bash
# grep (standard)
grep -r "pattern" .
grep -i "pattern" file.txt  # case-insensitive
grep -n "pattern" file.txt  # with line numbers
grep -A 5 "pattern" file.txt  # 5 lines after match
grep -B 5 "pattern" file.txt  # 5 lines before match

# ripgrep (if installed - faster and better)
rg "pattern"
rg -i "pattern"  # case-insensitive
rg -t rust "pattern"  # only Rust files
```

### Text Manipulation
```bash
# sed (stream editor) - macOS uses BSD sed
sed -i '' 's/old/new/g' file.txt  # note the '' for in-place edit
sed 's/pattern/replacement/' file.txt

# awk
awk '{print $1}' file.txt

# cut
cut -d',' -f1,3 file.csv
```

## Process Management

### Viewing Processes
```bash
# List processes
ps aux
ps aux | grep cargo

# Interactive process viewer
top
htop  # if installed (better)

# Activity Monitor (GUI)
open -a "Activity Monitor"
```

### Process Control
```bash
# Kill process
kill PID
kill -9 PID  # force kill
killall process_name

# Background/foreground
command &  # run in background
fg  # bring to foreground
bg  # continue in background
Ctrl+Z  # suspend foreground process
```

## Network

### Network Info
```bash
# IP address
ifconfig
ipconfig getifaddr en0  # specific interface

# Network connectivity
ping google.com
traceroute google.com

# DNS lookup
nslookup domain.com
dig domain.com

# Network statistics
netstat -an
lsof -i  # list open network connections
```

### Port Management
```bash
# Check what's using a port
lsof -i :8080
lsof -i tcp:3000

# Kill process using port
lsof -ti:8080 | xargs kill
```

## File Permissions

### Basic Permissions
```bash
# Change permissions
chmod +x script.sh  # make executable
chmod 644 file.txt  # rw-r--r--
chmod 755 dir/      # rwxr-xr-x

# Change ownership
chown user:group file
chown -R user:group directory/

# View permissions
ls -l
stat file.txt  # detailed info
```

### Extended Attributes (macOS-specific)
```bash
# List extended attributes
xattr -l file

# Remove quarantine attribute
xattr -d com.apple.quarantine file

# Clear all extended attributes
xattr -c file
```

## Disk & Storage

### Disk Usage
```bash
# Disk space
df -h
df -h /

# Directory size
du -sh directory/
du -h -d 1 .  # depth 1

# Sort by size
du -sh * | sort -h
```

### Disk Utility
```bash
# Verify disk
diskutil verifyVolume /
diskutil list

# Mount/unmount
diskutil mount diskName
diskutil unmount diskName
```

## Package Management

### Homebrew (common on macOS)
```bash
# Install package
brew install package-name

# Update Homebrew
brew update

# Upgrade packages
brew upgrade

# List installed
brew list

# Search packages
brew search pattern
```

### Mac App Store
```bash
# List updates
softwareupdate --list

# Install updates
softwareupdate --install --all
```

## System Information

### System Details
```bash
# macOS version
sw_vers
sw_vers -productVersion

# System profiler
system_profiler SPHardwareDataType
system_profiler SPSoftwareDataType

# Kernel info
uname -a
```

### Hardware Info
```bash
# CPU info
sysctl -n machdep.cpu.brand_string
sysctl hw

# Memory
top -l 1 | grep PhysMem

# Disk info
diskutil info /
```

## Environment & Shell

### Environment Variables
```bash
# View all
env
printenv

# Set variable
export VAR_NAME=value

# Shell config files
~/.zshrc     # Zsh (default on modern macOS)
~/.bashrc    # Bash
~/.profile   # Login shell
```

### Path Management
```bash
# View PATH
echo $PATH

# Add to PATH (in ~/.zshrc or ~/.bashrc)
export PATH="/usr/local/bin:$PATH"

# Which command
which cargo
which rustc
```

## Archives & Compression

### Tar
```bash
# Create archive
tar -czf archive.tar.gz directory/

# Extract archive
tar -xzf archive.tar.gz

# List contents
tar -tzf archive.tar.gz
```

### Zip
```bash
# Create zip
zip -r archive.zip directory/

# Extract zip
unzip archive.zip

# List contents
unzip -l archive.zip
```

## Clipboard (macOS-specific)

```bash
# Copy to clipboard
echo "text" | pbcopy
cat file.txt | pbcopy

# Paste from clipboard
pbpaste
pbpaste > file.txt
```

## Notifications (macOS-specific)

```bash
# Display notification
osascript -e 'display notification "Message" with title "Title"'

# Alert dialog
osascript -e 'display dialog "Message" with title "Title"'
```

## Xcode & iOS Development

### Xcode Command Line Tools
```bash
# Install command line tools
xcode-select --install

# Show active developer directory
xcode-select -p

# Switch Xcode version
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

### iOS Simulator
```bash
# List simulators
xcrun simctl list devices

# Boot simulator
xcrun simctl boot "iPad Pro 12.9-inch M2"

# Open Simulator app
open -a Simulator

# Install app
xcrun simctl install <device-uuid> path/to/app.app

# Launch app
xcrun simctl launch <device-uuid> bundle.id

# View logs
xcrun simctl spawn <device-uuid> log stream
```

### Physical Device
```bash
# List connected devices
xcrun devicectl list devices

# Install app
xcrun devicectl device install app --device <device-id> path/to/app.app

# Launch app
xcrun devicectl device process launch --device <device-id> bundle.id

# View logs
xcrun devicectl device stream log --device <device-id>
```

### Code Signing
```bash
# List signing identities
security find-identity -v -p codesigning

# Sign application
codesign -s "Developer ID" path/to/app.app

# Verify signature
codesign -vv path/to/app.app
```

## macOS-Specific Differences from Linux

### Key Differences
1. **sed**: Requires empty string for in-place edit: `sed -i '' ...`
2. **find**: Uses BSD find (slightly different options)
3. **date**: Different format options than GNU date
4. **readlink**: Use `greadlink` (if coreutils installed) for `-f` flag
5. **stat**: Different output format than GNU stat
6. **grep**: BSD grep (consider installing `ggrep` for GNU grep)

### GNU Tools via Homebrew
```bash
# Install GNU coreutils
brew install coreutils

# Then use with 'g' prefix
gls, gcp, gmv, grm, greadlink, gdate, etc.
```

## Useful macOS Shortcuts

### Terminal Shortcuts
- `Cmd+K` - Clear terminal
- `Cmd+T` - New tab
- `Cmd+N` - New window
- `Cmd+W` - Close tab
- `Cmd+,` - Preferences

### Command Line Shortcuts
- `Ctrl+A` - Beginning of line
- `Ctrl+E` - End of line
- `Ctrl+U` - Delete to beginning
- `Ctrl+K` - Delete to end
- `Ctrl+R` - Search history
- `Ctrl+C` - Cancel command
- `Ctrl+D` - Exit shell
- `Ctrl+Z` - Suspend process

## Quick Reference

### Most Common for Aspen Development
```bash
# Find Rust files
find . -name "*.rs"

# Search in Rust files
grep -r "pattern" crates/

# Check what's using a port
lsof -i :8080

# View disk space
df -h

# View process list
ps aux | grep cargo

# View logs
log stream --predicate 'process == "app"'

# Xcode simulators
xcrun simctl list devices available
```