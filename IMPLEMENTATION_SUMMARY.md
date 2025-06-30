# Glimpse Configuration Prompt Enhancement

## Overview
Modified the Rust application "glimpse" to improve the behavior around local `.glimpse` configuration files by tracking user preferences globally per project.

## Requirements Implemented
1. ✅ Stop showing the input prompt to save local config if the user has already said "no" once for the current project
2. ✅ Track this globally per project for the user
3. ✅ Still allow the existing save local config flag (`--config`) to work and write a `.glimpse` file

## Technical Implementation

### Files Modified

#### `src/config.rs`
- **Added imports**: `std::collections::HashSet`
- **New struct**: `GlobalConfig` with `declined_projects: HashSet<String>` field
- **New functions**:
  - `get_global_config_path()` - Returns path to `~/.config/glimpse/global.toml`
  - `load_global_config()` - Loads global config or returns default
  - `save_global_config()` - Saves global config to disk
  - `mark_project_declined()` - Adds a project path to the declined list
  - `is_project_declined()` - Checks if a project was previously declined

#### `src/main.rs`
- **Updated imports**: Added new functions from config module
- **Modified prompt logic**:
  - Check `is_project_declined(&root_dir)` before showing prompt
  - Accept both "y"/"yes" and "n"/"no" responses (previously only "y")
  - Call `mark_project_declined(&root_dir)` when user says "no"
  - Show warning if saving declined preference fails

### Key Technical Details
- Uses canonical paths to ensure consistent project identification across different working directories
- Stores declined projects in `~/.config/glimpse/global.toml` (respects XDG_CONFIG_HOME)
- Preserves existing `--config` flag functionality to force save local config
- Gracefully handles errors when saving global preferences

## Behavior Changes

### Before Implementation
- Always showed prompt when using custom options
- Only accepted "y" to save config, anything else was treated as "no"
- No memory of user preferences across runs

### After Implementation
- **First time with custom options**: Shows prompt as before
- **User says "yes"**: Saves `.glimpse` file as before
- **User says "no"**: Saves this preference globally and won't prompt again for this project
- **Using `--config` flag**: Always saves `.glimpse` file regardless of previous declined status
- **Different projects**: Each project tracked independently

## Testing Results
✅ Build successful after resolving OpenSSL dependencies  
✅ Prompt appears on first run with custom options  
✅ Prompt is skipped on subsequent runs after declining  
✅ Global config file created at `~/.config/glimpse/global.toml`  
✅ `--config` flag works to force save local config even after declining  
✅ Per-project tracking works correctly (different projects show prompt independently)  
✅ Existing functionality preserved (help, other flags, etc.)

## Dependencies Added
- No new Rust dependencies required
- System dependencies resolved: `libssl-dev` and `pkg-config` for OpenSSL support

## Files Created/Modified
- `src/config.rs` - Enhanced with global config management
- `src/main.rs` - Updated prompt logic
- `~/.config/glimpse/global.toml` - New global config file (created at runtime)

## Backward Compatibility
- All existing functionality preserved
- Existing `.glimpse` files continue to work
- All command-line flags work as before
- No breaking changes to the API or user interface