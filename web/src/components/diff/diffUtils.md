# Diff Utilities Module

This module provides utility functions for parsing and processing unified diff format text for the GitHub-style diff viewer.

## Functions

### `parseDiffText(diffText)`

Parses unified diff format text into structured diff objects using `react-diff-view`'s `parseDiff` function.

**Parameters:**
- `diffText` (string): Unified diff format text (e.g., from git diff)

**Returns:**
- Array of parsed diff file objects, or empty array on error

**Features:**
- Input validation (handles null, undefined, non-string, empty inputs)
- Error handling with graceful degradation
- Console logging for debugging

**Example:**
```javascript
const diffText = `--- a/config.toml
+++ b/config.toml
@@ -1,3 +1,4 @@
 [server]
-port = 8080
+port = 8080
+host = "0.0.0.0"`;

const files = parseDiffText(diffText);
// Returns array with parsed diff structure
```

### `getLanguageFromFileName(fileName)`

Maps file extensions to programming language identifiers for syntax highlighting.

**Parameters:**
- `fileName` (string): File name or path

**Returns:**
- Language identifier string (e.g., 'javascript', 'toml', 'json')
- Returns 'text' for unknown or unsupported file types

**Supported Languages:**
- Configuration: toml, json, yaml, yml, xml, ini
- JavaScript/TypeScript: js, jsx, ts, tsx, mjs, cjs
- Web: html, css, scss, sass, less
- Python: py, pyw
- Rust: rs
- Go: go
- And many more...

**Features:**
- Case-insensitive extension matching
- Handles file paths (extracts extension correctly)
- Handles files with multiple dots
- Safe fallback to 'text' for unknown types

**Example:**
```javascript
getLanguageFromFileName('config.toml')           // Returns 'toml'
getLanguageFromFileName('src/App.jsx')           // Returns 'jsx'
getLanguageFromFileName('README.md')             // Returns 'markdown'
getLanguageFromFileName('unknown.xyz')           // Returns 'text'
```

## Testing

The module includes comprehensive unit tests covering:
- Valid diff parsing
- Error handling (null, undefined, empty, invalid inputs)
- File addition/deletion detection
- Language identification for 20+ file types
- Edge cases (no extension, multiple dots, case sensitivity)

Run tests with:
```bash
npm test -- src/components/diff/diffUtils.test.js
```

## Usage in Components

```javascript
import { parseDiffText, getLanguageFromFileName } from './diffUtils';

// Parse diff from backend
const files = parseDiffText(backendDiffText);

// Get language for syntax highlighting
files.forEach(file => {
  const language = getLanguageFromFileName(file.newPath || file.oldPath);
  // Apply syntax highlighting based on language
});
```

## Requirements Validation

This module satisfies the following requirements:
- **Requirement 1.1**: Parses unified diff format for display
- **Requirement 3.1**: Handles backend diff data format
- **Requirement 5.1**: Identifies file types for syntax highlighting
- **Requirement 5.2**: Supports configuration file formats (toml, json, yaml, xml)
