# Diff Viewer Dependencies

This directory contains components for the GitHub-style diff viewer feature.

## Installed Dependencies

The following npm packages have been installed and configured:

### Core Dependencies

1. **react-diff-view** (^3.0.0)
   - Main library for rendering diff views
   - Provides `Diff`, `Hunk`, and `parseDiff` components/functions
   - CSS: Import with `import 'react-diff-view/style/index.css'`

2. **diff** (^5.0.0)
   - Library for generating unified diff format
   - Provides `createTwoFilesPatch`, `structuredPatch`, etc.
   - Used to generate diff text from old/new content

3. **refractor** (^4.0.0)
   - Syntax highlighting library based on Prism
   - Used with react-diff-view's tokenize feature
   - Supports multiple programming languages

4. **prismjs** (^1.29.0)
   - Syntax highlighting engine
   - Used by refractor for language definitions
   - Provides themes and language grammars

## Vite Configuration

Vite is already configured to handle:
- CSS imports (including from node_modules)
- ES modules
- React components with SWC

No additional Vite configuration was needed for these dependencies.

## Usage Example

```javascript
import React from 'react';
import { parseDiff, Diff, Hunk } from 'react-diff-view';
import { createTwoFilesPatch } from 'diff';
import 'react-diff-view/style/index.css';

function MyDiffViewer({ oldContent, newContent, fileName }) {
  // Generate unified diff format
  const diffText = createTwoFilesPatch(
    fileName,
    fileName,
    oldContent,
    newContent,
    'Old Version',
    'New Version'
  );
  
  // Parse the diff
  const files = parseDiff(diffText);
  
  return (
    <div>
      {files.map((file, index) => (
        <Diff 
          key={index} 
          viewType="split" 
          diffType={file.type} 
          hunks={file.hunks}
        >
          {(hunks) => hunks.map((hunk) => (
            <Hunk key={hunk.content} hunk={hunk} />
          ))}
        </Diff>
      ))}
    </div>
  );
}
```

## Next Steps

The dependencies are now installed and ready for use. The next tasks will involve:
1. Creating diff parsing utility functions
2. Building the DiffViewer component
3. Implementing syntax highlighting
4. Adding line numbers and other features

## Verification

A test component `DiffViewerTest.jsx` has been created to verify the dependencies work correctly. You can import and use it to test the installation.
