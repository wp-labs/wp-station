/**
 * Diff parsing and utility functions for the GitHub-style diff viewer
 * 
 * This module provides functions to parse unified diff format and extract
 * language information from file names.
 */

import { parseDiff } from 'react-diff-view';

/**
 * Parse unified diff format text into structured diff objects
 * 
 * @param {string} diffText - Unified diff format text (e.g., from git diff)
 * @returns {Array<Object>} Array of parsed diff file objects, or empty array on error
 * 
 * Each returned object contains:
 * - oldRevision: string - Old file revision
 * - newRevision: string - New file revision
 * - oldPath: string - Old file path
 * - newPath: string - New file path
 * - type: string - Change type ('add', 'delete', 'modify', 'rename', 'copy')
 * - hunks: Array - Array of diff hunks with changes
 * 
 * @example
 * const diffText = `--- a/config.toml
 * +++ b/config.toml
 * @@ -1,3 +1,4 @@
 *  [server]
 * -port = 8080
 * +port = 8080
 * +host = "0.0.0.0"`;
 * 
 * const files = parseDiffText(diffText);
 * // Returns array of parsed diff objects
 */
export function parseDiffText(diffText) {
  try {
    // Validate input
    if (!diffText || typeof diffText !== 'string') {
      console.error('parseDiffText: Invalid input - diffText must be a non-empty string');
      return [];
    }

    // Trim whitespace
    const trimmedText = diffText.trim();
    
    if (trimmedText.length === 0) {
      console.warn('parseDiffText: Empty diff text provided');
      return [];
    }

    // Parse using react-diff-view's parseDiff function
    const parsedFiles = parseDiff(trimmedText);
    
    // Validate parsed result
    if (!Array.isArray(parsedFiles)) {
      console.error('parseDiffText: parseDiff returned non-array result');
      return [];
    }

    return parsedFiles;
    
  } catch (error) {
    // Handle parsing errors gracefully
    console.error('parseDiffText: Failed to parse diff text', {
      error: error.message,
      stack: error.stack
    });
    
    // Return empty array to allow graceful degradation
    return [];
  }
}

/**
 * Get programming language identifier from file name
 * 
 * Maps file extensions to language identifiers used by syntax highlighters
 * like Prism/Refractor. Supports common configuration and programming languages.
 * 
 * @param {string} fileName - File name or path (e.g., 'config.toml', 'src/main.js')
 * @returns {string} Language identifier (e.g., 'javascript', 'toml', 'json')
 *                   Returns 'text' for unknown or unsupported file types
 * 
 * @example
 * getLanguageFromFileName('config.toml') // Returns 'toml'
 * getLanguageFromFileName('package.json') // Returns 'json'
 * getLanguageFromFileName('README.md') // Returns 'markdown'
 * getLanguageFromFileName('unknown.xyz') // Returns 'text'
 */
export function getLanguageFromFileName(fileName) {
  // Validate input
  if (!fileName || typeof fileName !== 'string') {
    return 'text';
  }

  // Extract file extension (handle paths like 'src/config.toml')
  const lastDotIndex = fileName.lastIndexOf('.');
  
  if (lastDotIndex === -1 || lastDotIndex === fileName.length - 1) {
    // No extension or ends with dot
    return 'text';
  }

  const extension = fileName.slice(lastDotIndex + 1).toLowerCase();

  // Map extensions to language identifiers
  // Based on common file types and Prism language identifiers
  const languageMap = {
    // Configuration files
    'toml': 'toml',
    'json': 'json',
    'yaml': 'yaml',
    'yml': 'yaml',
    'xml': 'xml',
    'ini': 'ini',
    'conf': 'nginx', // Common config format
    'config': 'text',
    
    // JavaScript/TypeScript
    'js': 'javascript',
    'jsx': 'jsx',
    'ts': 'typescript',
    'tsx': 'tsx',
    'mjs': 'javascript',
    'cjs': 'javascript',
    
    // Web
    'html': 'html',
    'htm': 'html',
    'css': 'css',
    'scss': 'scss',
    'sass': 'sass',
    'less': 'less',
    
    // Python
    'py': 'python',
    'pyw': 'python',
    
    // Ruby
    'rb': 'ruby',
    'rake': 'ruby',
    
    // PHP
    'php': 'php',
    
    // Java/Kotlin
    'java': 'java',
    'kt': 'kotlin',
    'kts': 'kotlin',
    
    // C/C++
    'c': 'c',
    'h': 'c',
    'cpp': 'cpp',
    'cc': 'cpp',
    'cxx': 'cpp',
    'hpp': 'cpp',
    
    // C#
    'cs': 'csharp',
    
    // Go
    'go': 'go',
    
    // Rust
    'rs': 'rust',
    
    // Shell
    'sh': 'bash',
    'bash': 'bash',
    'zsh': 'bash',
    
    // SQL
    'sql': 'sql',
    
    // Markdown
    'md': 'markdown',
    'markdown': 'markdown',
    
    // Docker
    'dockerfile': 'docker',
    
    // GraphQL
    'graphql': 'graphql',
    'gql': 'graphql',
    
    // Other
    'txt': 'text',
    'log': 'text',
  };

  // Return mapped language or 'text' as fallback
  return languageMap[extension] || 'text';
}

/**
 * Generate unified diff text from old and new content
 * 
 * This is a helper function that can be used when the backend provides
 * raw file contents instead of unified diff format.
 * 
 * @param {string} oldContent - Old version of the file content
 * @param {string} newContent - New version of the file content
 * @param {string} fileName - File name for the diff header
 * @returns {string} Unified diff format text
 * 
 * Note: This function requires the 'diff' library to be imported separately
 * when needed, to avoid unnecessary bundle size.
 */
export function generateDiffText(oldContent, newContent, fileName) {
  // This is a placeholder - actual implementation would use the 'diff' library
  // Import dynamically when needed: import { createTwoFilesPatch } from 'diff';
  throw new Error('generateDiffText: Not implemented - import and use createTwoFilesPatch from "diff" library directly');
}

/**
 * Detect if a hunk should be collapsible based on the number of unchanged context lines
 * 
 * A hunk is considered collapsible if it has a large number of consecutive unchanged
 * (normal) lines in the middle, which can be hidden to improve readability.
 * 
 * @param {Object} hunk - Hunk object from parsed diff
 * @param {number} threshold - Minimum number of consecutive normal lines to trigger collapse (default: 10)
 * @returns {boolean} True if the hunk should be collapsible
 */
export function isHunkCollapsible(hunk, threshold = 10) {
  if (!hunk || !Array.isArray(hunk.changes)) {
    return false;
  }

  // Count consecutive normal (unchanged) lines
  let maxConsecutiveNormal = 0;
  let currentConsecutiveNormal = 0;

  for (const change of hunk.changes) {
    if (change.type === 'normal' || change.isNormal) {
      currentConsecutiveNormal++;
      maxConsecutiveNormal = Math.max(maxConsecutiveNormal, currentConsecutiveNormal);
    } else {
      currentConsecutiveNormal = 0;
    }
  }

  return maxConsecutiveNormal >= threshold;
}

/**
 * Split a hunk into segments for collapsing
 * 
 * Divides a hunk into visible segments (with changes) and collapsible segments
 * (long runs of unchanged context lines). This allows selective expansion of
 * collapsed sections.
 * 
 * @param {Object} hunk - Hunk object from parsed diff
 * @param {number} contextLines - Number of context lines to keep visible around changes (default: 3)
 * @param {number} collapseThreshold - Minimum consecutive normal lines to collapse (default: 10)
 * @returns {Array<Object>} Array of segments with type 'visible' or 'collapsible'
 * 
 * Each segment contains:
 * - type: 'visible' | 'collapsible'
 * - changes: Array of change objects
 * - lineCount: Number of lines in the segment
 */
export function splitHunkIntoSegments(hunk, contextLines = 3, collapseThreshold = 10) {
  if (!hunk || !Array.isArray(hunk.changes) || hunk.changes.length === 0) {
    return [];
  }

  const segments = [];
  let currentNormalRun = [];
  
  for (let i = 0; i < hunk.changes.length; i++) {
    const change = hunk.changes[i];
    const isNormal = change.type === 'normal' || change.isNormal;

    if (isNormal) {
      currentNormalRun.push(change);
    } else {
      // We hit a change line, process any accumulated normal lines
      if (currentNormalRun.length > 0) {
        processNormalRun(currentNormalRun, segments, contextLines, collapseThreshold, i === hunk.changes.length - 1);
        currentNormalRun = [];
      }
      
      // Add the change line to a visible segment
      if (segments.length === 0 || segments[segments.length - 1].type === 'collapsible') {
        segments.push({ type: 'visible', changes: [change], lineCount: 1 });
      } else {
        segments[segments.length - 1].changes.push(change);
        segments[segments.length - 1].lineCount++;
      }
    }
  }
  
  // Process any remaining normal lines at the end
  if (currentNormalRun.length > 0) {
    processNormalRun(currentNormalRun, segments, contextLines, collapseThreshold, true);
  }
  
  // If we only have one segment, return it as-is
  if (segments.length === 1) {
    return segments;
  }
  
  return segments;
}

/**
 * Helper function to process a run of consecutive normal lines
 * @private
 */
function processNormalRun(normalRun, segments, contextLines, collapseThreshold, isEnd) {
  const runLength = normalRun.length;
  
  // If this is at the start (no segments yet)
  if (segments.length === 0) {
    if (runLength <= contextLines) {
      // Too short to collapse, add as visible
      segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
    } else {
      // Keep last contextLines visible, collapse the rest
      const collapsibleCount = runLength - contextLines;
      if (collapsibleCount >= collapseThreshold) {
        segments.push({ 
          type: 'collapsible', 
          changes: normalRun.slice(0, collapsibleCount), 
          lineCount: collapsibleCount 
        });
        segments.push({ 
          type: 'visible', 
          changes: normalRun.slice(collapsibleCount), 
          lineCount: contextLines 
        });
      } else {
        segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
      }
    }
    return;
  }
  
  // If this is at the end
  if (isEnd) {
    if (runLength <= contextLines) {
      // Too short to collapse, add to last segment or create new visible
      if (segments[segments.length - 1].type === 'visible') {
        segments[segments.length - 1].changes.push(...normalRun);
        segments[segments.length - 1].lineCount += runLength;
      } else {
        segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
      }
    } else {
      // Keep first contextLines visible, collapse the rest
      const collapsibleCount = runLength - contextLines;
      if (collapsibleCount >= collapseThreshold) {
        if (segments[segments.length - 1].type === 'visible') {
          segments[segments.length - 1].changes.push(...normalRun.slice(0, contextLines));
          segments[segments.length - 1].lineCount += contextLines;
        } else {
          segments.push({ 
            type: 'visible', 
            changes: normalRun.slice(0, contextLines), 
            lineCount: contextLines 
          });
        }
        segments.push({ 
          type: 'collapsible', 
          changes: normalRun.slice(contextLines), 
          lineCount: collapsibleCount 
        });
      } else {
        if (segments[segments.length - 1].type === 'visible') {
          segments[segments.length - 1].changes.push(...normalRun);
          segments[segments.length - 1].lineCount += runLength;
        } else {
          segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
        }
      }
    }
    return;
  }
  
  // In the middle - keep contextLines on both sides, collapse the middle
  const totalContext = contextLines * 2;
  
  if (runLength <= totalContext) {
    // Too short to collapse, add to last segment
    if (segments[segments.length - 1].type === 'visible') {
      segments[segments.length - 1].changes.push(...normalRun);
      segments[segments.length - 1].lineCount += runLength;
    } else {
      segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
    }
  } else {
    const collapsibleCount = runLength - totalContext;
    if (collapsibleCount >= collapseThreshold) {
      // Add first contextLines to previous visible segment
      if (segments[segments.length - 1].type === 'visible') {
        segments[segments.length - 1].changes.push(...normalRun.slice(0, contextLines));
        segments[segments.length - 1].lineCount += contextLines;
      } else {
        segments.push({ 
          type: 'visible', 
          changes: normalRun.slice(0, contextLines), 
          lineCount: contextLines 
        });
      }
      
      // Add collapsible middle
      segments.push({ 
        type: 'collapsible', 
        changes: normalRun.slice(contextLines, runLength - contextLines), 
        lineCount: collapsibleCount 
      });
      
      // Add last contextLines as new visible segment
      segments.push({ 
        type: 'visible', 
        changes: normalRun.slice(runLength - contextLines), 
        lineCount: contextLines 
      });
    } else {
      // Not enough to collapse, add all to visible
      if (segments[segments.length - 1].type === 'visible') {
        segments[segments.length - 1].changes.push(...normalRun);
        segments[segments.length - 1].lineCount += runLength;
      } else {
        segments.push({ type: 'visible', changes: [...normalRun], lineCount: runLength });
      }
    }
  }
}
