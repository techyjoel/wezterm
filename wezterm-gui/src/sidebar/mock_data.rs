//! Mock data generation for testing and development of the AI sidebar.
//!
//! This module provides functions to populate the sidebar with sample data
//! for testing various UI components and interactions.

use crate::sidebar::ai_sidebar::{
    ActivityItem, AgentMode, AiSidebar, CommandStatus, CurrentGoal, CurrentSuggestion,
};
use std::time::{Duration, SystemTime};

impl AiSidebar {
    /// Populate the sidebar with mock data for testing purposes
    pub fn populate_mock_data(&mut self) {
        // Set a current goal
        let goal_text = "Fix the build errors in the project".to_string();
        log::debug!(
            "GOAL TEXT DEBUG: Setting mock goal text: '{}', len={}",
            goal_text,
            goal_text.len()
        );
        self.current_goal = Some(CurrentGoal {
            text: goal_text,
            is_ai_inferred: true,
            is_confirmed: false,
            is_editing: false,
            edit_text: String::new(),
        });

        // Set a current suggestion with very long content to test scrolling
        let test_content = r#"It looks like the linker couldn't find OpenSSL. This is a common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide a comprehensive guide to resolving this issue.

## Quick Solution

Run the following command to install OpenSSL:

```bash
brew install openssl@3
```

## If That Doesn't Work

You may need to set environment variables to help the build system find OpenSSL:

```bash
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
```

## Common Issues

1. **Wrong OpenSSL version**: Some projects require openssl@1.1 instead of openssl@3
2. **Multiple OpenSSL installations**: Check `brew list | grep openssl` to see all versions
3. **Architecture mismatch**: On M1 Macs, ensure you're using the right architecture
4. **Missing pkg-config**: Install with `brew install pkg-config`
5. **Incorrect paths**: Verify paths with `brew --prefix openssl@3`

## Detailed Troubleshooting Steps

### Step 1: Check Current Installation
First, let's check what OpenSSL versions you have installed:

```bash
brew list | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl
which openssl
openssl version
```

### Step 2: Clean Installation
If you have conflicts, clean up first:

```bash
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew cleanup
```

### Step 3: Fresh Install
Install the required version:

```bash
brew install openssl@3
brew link openssl@3 --force
```

### Step 4: Verify Installation
Check that everything is properly installed:

```bash
brew test openssl@3
pkg-config --libs openssl
```

### Step 5: Configure Your Shell
Add these to your shell configuration file (~/.zshrc or ~/.bashrc):

```bash
# OpenSSL Configuration
export PATH="/opt/homebrew/opt/openssl@3/bin:$PATH"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
```

### Step 6: Alternative Solutions

#### Using MacPorts
If Homebrew doesn't work, try MacPorts:

```bash
sudo port install openssl
sudo port select --set openssl openssl3
```

#### Building from Source
As a last resort, build OpenSSL from source:

```bash
wget https://www.openssl.org/source/openssl-3.0.7.tar.gz
tar -xf openssl-3.0.7.tar.gz
cd openssl-3.0.7
./config --prefix=/usr/local/openssl --openssldir=/usr/local/openssl
make
sudo make install
```

## Platform-Specific Notes

### macOS Monterey and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries. You may need to:

1. Disable System Integrity Protection (not recommended)
2. Use a different crypto library
3. Explicitly specify OpenSSL paths in your build configuration

### M1/M2 Mac Considerations
On Apple Silicon, paths differ:
- Intel: `/usr/local/opt/openssl@3`
- Apple Silicon: `/opt/homebrew/opt/openssl@3`

## Related Issues
- libssl-dev on Linux: `sudo apt-get install libssl-dev`
- Windows: Use vcpkg or download prebuilt binaries
- Docker: Add `RUN apk add --no-cache openssl-dev` to Dockerfile

This should resolve most OpenSSL-related build issues. If problems persist, check your project's specific requirements.

If you're still having issues:

1. Clean your build directory: `make clean`
2. Check your PATH: `echo $PATH`
3. Verify OpenSSL installation: `brew info openssl@3`
4. Try linking manually: `brew link openssl@3 --force`

## References

- [Homebrew OpenSSL Formula](https://formulae.brew.sh/formula/openssl@3)
- [Common macOS linking issues](https://github.com/openssl/openssl/issues)

This should resolve your OpenSSL linking error. If problems persist, check your project's specific build documentation."#;

        self.current_suggestion = Some(CurrentSuggestion {
            title: "Install missing dependency".to_string(),
            content: test_content.to_string(),
            has_action: true,
            action_type: Some("run".to_string()),
        });

        // Add some activity items
        let now = SystemTime::now();
        self.activity_log.push(ActivityItem::Command {
            id: "cmd1".to_string(),
            command: "make (~/project)".to_string(),
            output: Some("Error: OpenSSL not found".to_string()),
            pane_id: Some("pane1".to_string()),
            status: CommandStatus::Failed(1),
            timestamp: now - Duration::from_secs(60),
            expanded: true, // Make it expanded to see if that's the tall content
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat1".to_string(),
            message: "I'm trying to compile my Rust project but getting linker errors about OpenSSL. I've tried installing it before but it doesn't seem to be working. Can you help me understand what's going wrong and how to fix it properly?".to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(30),
        });

        // Add AI response with long markdown content to test text wrapping
        self.activity_log.push(ActivityItem::Chat {
            id: "chat2".to_string(),
            message: r#"I see you're getting an **OpenSSL error**. This is a very common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide you with a comprehensive guide to resolve this issue on macOS.

## Quick Solution (Try This First)

The fastest way to resolve this is usually:

1. First, *check* if OpenSSL is installed:
   ```bash
   brew list openssl
   brew list | grep openssl
   ```

2. If **not installed**, run:
   ```bash
   brew install openssl@3
   # or for older projects:
   brew install openssl@1.1
   ```

3. Then set the environment variables:
   ```bash
   export OPENSSL_DIR=$(brew --prefix openssl)
   export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
   export LDFLAGS="-L$OPENSSL_DIR/lib"
   export CPPFLAGS="-I$OPENSSL_DIR/include"
   # This is a very long line that should definitely trigger horizontal scrolling in the code block - it contains many characters and should exceed the width of the sidebar
   ```

4. Try running `make` again.

## Detailed Troubleshooting

If the quick solution doesn't work, here are more comprehensive steps:

### Step 1: Verify Your System
First, let's understand your environment:
```bash
# Check macOS version
sw_vers -productVersion

# Check architecture (Intel vs Apple Silicon)
uname -m

# Check Homebrew installation
brew --version
brew config
```

### Step 2: Clean Up Existing Installations
Sometimes conflicts arise from multiple OpenSSL installations:
```bash
# List all OpenSSL installations
brew list | grep openssl
ls -la /usr/local/opt/ | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl

# If you have conflicts, uninstall all versions
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew uninstall --ignore-dependencies openssl
```

### Step 3: Install the Correct Version
Different projects require different OpenSSL versions:
```bash
# For modern projects (OpenSSL 3.x)
brew install openssl@3

# For older projects (OpenSSL 1.1)
brew install openssl@1.1

# Force link if needed
brew link openssl@3 --force
```

### Step 4: Configure pkg-config
The pkg-config tool helps compilers find libraries:
```bash
# Install pkg-config if missing
brew install pkg-config

# Verify it can find OpenSSL
pkg-config --modversion openssl
pkg-config --libs openssl
pkg-config --cflags openssl
```

### Alternative Solution
If the above doesn't work, you might need to:
```bash
# Install pkg-config
brew install pkg-config

# Or try using the system's built-in LibreSSL
export LDFLAGS="-L/usr/lib"
export CPPFLAGS="-I/usr/include"
```

## Platform-Specific Considerations

### Apple Silicon (M1/M2) Macs
Paths differ on Apple Silicon:
- Intel Macs: `/usr/local/opt/openssl`
- Apple Silicon: `/opt/homebrew/opt/openssl`

### macOS Ventura and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries, which can cause additional complications.

This comprehensive guide should resolve most OpenSSL linking issues on macOS!"#.to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(20),
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat3".to_string(),
            message: "Great! That worked. Now I'm seeing some warnings about deprecated functions."
                .to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(10),
        });

        // Add a Python example with indentation to test code block rendering
        self.activity_log.push(ActivityItem::Chat {
            id: "chat4".to_string(),
            message: r#"Here's a Python example showing proper error handling with indentation:

```python
def process_data(filename):
    """Process data from a file with proper error handling."""
    try:
        with open(filename, 'r') as file:
            data = file.read()
            # Process each line
            for line in data.splitlines():
                if line.strip():  # Skip empty lines
                    result = parse_line(line)
                    if result:
                        yield result
    except FileNotFoundError:
        print(f"Error: File '{filename}' not found")
        return None
    except PermissionError:
        print(f"Error: Permission denied for '{filename}'")
        return None
    finally:
        print("Processing complete")
```

This example demonstrates:
- Function definition with docstring
- Context manager (`with` statement)
- Nested indentation levels (up to 5 levels deep)
- Error handling with multiple `except` blocks
- The `finally` clause for cleanup"#
                .to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(5),
        });

        // Add more mock items to test scrolling
        for i in 0..20 {
            if i % 3 == 0 {
                self.activity_log.push(ActivityItem::Command {
                    id: format!("cmd{}", i + 10),
                    command: format!("test command {}", i),
                    output: Some(format!("Output for command {}", i)),
                    pane_id: Some("pane1".to_string()),
                    status: if i % 2 == 0 {
                        CommandStatus::Success
                    } else {
                        CommandStatus::Failed(1)
                    },
                    timestamp: now - Duration::from_secs(300 + i * 60),
                    expanded: false,
                });
            } else {
                self.activity_log.push(ActivityItem::Chat {
                    id: format!("chat{}", i + 10),
                    message: format!(
                        "Test message {} from {}",
                        i,
                        if i % 2 == 0 { "user" } else { "AI" }
                    ),
                    is_user: i % 2 == 0,
                    timestamp: now - Duration::from_secs(300 + i * 60),
                });
            }
        }

        self.agent_mode = AgentMode::Thinking;

        // Clear code block registry since we've replaced all content
        self.clear_code_block_registry();
    }
}
