/**
 * DiffViewer - GitHub-style diff viewer component
 * 
 * This component renders file diffs in a GitHub-style interface using react-diff-view.
 * It supports both split and unified view modes, syntax highlighting, and visual
 * indicators for different change types.
 */

import React, { useState } from 'react';
import { Diff, Hunk } from 'react-diff-view';
import 'react-diff-view/style/index.css';
import './DiffViewer.css';
import { splitHunkIntoSegments } from './diffUtils';

/**
 * Get change type badge configuration
 * @param {string} changeType - Type of change: 'add', 'delete', 'modify', 'rename'
 * @returns {Object} Badge configuration with label and className
 */
function getChangeTypeBadge(changeType) {
  const badges = {
    add: { label: 'Added', className: 'change-badge-add' },
    delete: { label: 'Deleted', className: 'change-badge-delete' },
    modify: { label: 'Modified', className: 'change-badge-modify' },
    rename: { label: 'Renamed', className: 'change-badge-rename' }
  };
  
  return badges[changeType] || { label: 'Changed', className: 'change-badge-default' };
}

/**
 * FileHeader - Renders the file header with path and change type badge
 */
function FileHeader({ file, changeType, oldPath }) {
  const badge = getChangeTypeBadge(changeType);
  
  return (
    <div className="diff-file-header">
      <div className="diff-file-path">
        {changeType === 'rename' && oldPath ? (
          <div className="diff-file-rename">
            <span className="diff-file-old-path">{oldPath}</span>
            <span className="diff-file-rename-arrow"> → </span>
            <span className="diff-file-new-path">{file.newPath}</span>
          </div>
        ) : (
          <span className="diff-file-name">{file.newPath || file.oldPath}</span>
        )}
      </div>
      <span className={`change-badge ${badge.className}`}>
        {badge.label}
      </span>
    </div>
  );
}

/**
 * CollapsedSegment - Renders a collapsed segment with expand buttons
 * Supports full expansion and partial expansion (up/down)
 * Renders as a table row to maintain valid HTML structure within diff tables
 */
function CollapsedSegment({ segment, onExpand, onExpandUp, onExpandDown, viewType }) {
  const lineCount = segment.lineCount;
  const showPartialExpand = lineCount > 20; // Show partial expand for large segments
  
  // Render as a tbody with a single row to maintain table structure
  return (
    <tbody className="diff-collapsed-segment-tbody">
      <tr className="diff-collapsed-segment-row">
        <td colSpan="100" className="diff-collapsed-segment">
          {showPartialExpand ? (
            <div className="diff-expand-buttons-group">
              <button 
                className="diff-expand-button diff-expand-button-partial"
                onClick={onExpandUp}
                aria-label="Expand 10 lines up"
                title="Expand 10 lines up"
              >
                <svg 
                  className="diff-expand-icon" 
                  width="16" 
                  height="16" 
                  viewBox="0 0 16 16"
                  fill="currentColor"
                >
                  <path d="M3.47 7.78a.75.75 0 0 1 0-1.06l4.25-4.25a.75.75 0 0 1 1.06 0l4.25 4.25a.75.75 0 0 1-1.06 1.06L8.75 4.56v7.69a.75.75 0 0 1-1.5 0V4.56L4.53 7.78a.75.75 0 0 1-1.06 0Z" />
                </svg>
                <span className="diff-expand-text">Expand up</span>
              </button>
              
              <button 
                className="diff-expand-button"
                onClick={onExpand}
                aria-label={`Expand all ${lineCount} hidden lines`}
                title={`Expand all ${lineCount} lines`}
              >
                <svg 
                  className="diff-expand-icon" 
                  width="16" 
                  height="16" 
                  viewBox="0 0 16 16"
                  fill="currentColor"
                >
                  <path d="M8 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM1.5 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm13 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
                </svg>
                <span className="diff-expand-text">
                  {lineCount} hidden {lineCount === 1 ? 'line' : 'lines'}
                </span>
              </button>
              
              <button 
                className="diff-expand-button diff-expand-button-partial"
                onClick={onExpandDown}
                aria-label="Expand 10 lines down"
                title="Expand 10 lines down"
              >
                <svg 
                  className="diff-expand-icon" 
                  width="16" 
                  height="16" 
                  viewBox="0 0 16 16"
                  fill="currentColor"
                >
                  <path d="M12.53 8.22a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L3.47 9.28a.75.75 0 0 1 1.06-1.06l2.72 2.72V3.25a.75.75 0 0 1 1.5 0v7.69l2.72-2.72a.75.75 0 0 1 1.06 0Z" />
                </svg>
                <span className="diff-expand-text">Expand down</span>
              </button>
            </div>
          ) : (
            <button 
              className="diff-expand-button"
              onClick={onExpand}
              aria-label={`Expand ${lineCount} hidden lines`}
            >
              <svg 
                className="diff-expand-icon" 
                width="16" 
                height="16" 
                viewBox="0 0 16 16"
                fill="currentColor"
              >
                <path d="M8 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM1.5 9a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm13 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" />
              </svg>
              <span className="diff-expand-text">
                Expand {lineCount} hidden {lineCount === 1 ? 'line' : 'lines'}
              </span>
            </button>
          )}
        </td>
      </tr>
    </tbody>
  );
}

/**
 * CollapsibleHunk - Renders a hunk with collapsible segments
 * Supports full and partial expansion of collapsed segments
 */
function CollapsibleHunk({ hunk, fileIndex, hunkIndex, viewType }) {
  const [expandedSegments, setExpandedSegments] = useState(new Set());
  const [partiallyExpandedSegments, setPartiallyExpandedSegments] = useState(new Map());
  
  // Split hunk into segments
  const segments = splitHunkIntoSegments(hunk, 3, 10);
  
  // If there are no collapsible segments, render normally
  const hasCollapsible = segments.some(seg => seg.type === 'collapsible');
  
  if (!hasCollapsible) {
    return <Hunk key={`${fileIndex}-hunk-${hunkIndex}`} hunk={hunk} />;
  }
  
  // Handle full expand for a specific segment
  const handleExpand = (segmentIndex) => {
    setExpandedSegments(prev => {
      const newSet = new Set(prev);
      newSet.add(segmentIndex);
      return newSet;
    });
    // Remove partial expansion when fully expanded
    setPartiallyExpandedSegments(prev => {
      const newMap = new Map(prev);
      newMap.delete(segmentIndex);
      return newMap;
    });
  };
  
  // Handle partial expand up (show first 10 lines)
  const handleExpandUp = (segmentIndex) => {
    setPartiallyExpandedSegments(prev => {
      const newMap = new Map(prev);
      const current = newMap.get(segmentIndex) || { top: 0, bottom: 0 };
      newMap.set(segmentIndex, { ...current, top: Math.min(current.top + 10, segments[segmentIndex].changes.length) });
      return newMap;
    });
  };
  
  // Handle partial expand down (show last 10 lines)
  const handleExpandDown = (segmentIndex) => {
    setPartiallyExpandedSegments(prev => {
      const newMap = new Map(prev);
      const current = newMap.get(segmentIndex) || { top: 0, bottom: 0 };
      newMap.set(segmentIndex, { ...current, bottom: Math.min(current.bottom + 10, segments[segmentIndex].changes.length) });
      return newMap;
    });
  };
  
  // Render segments - return a fragment to avoid invalid HTML structure
  return (
    <>
      {segments.map((segment, segmentIndex) => {
        const isExpanded = expandedSegments.has(segmentIndex);
        const partialExpansion = partiallyExpandedSegments.get(segmentIndex);
        
        if (segment.type === 'collapsible' && !isExpanded) {
          // Check if we should show partial expansion
          if (partialExpansion && (partialExpansion.top > 0 || partialExpansion.bottom > 0)) {
            const { top, bottom } = partialExpansion;
            const totalShown = top + bottom;
            
            // If we've shown everything, just expand fully
            if (totalShown >= segment.changes.length) {
              handleExpand(segmentIndex);
              return null;
            }
            
            // Show top portion
            const topChanges = segment.changes.slice(0, top);
            const bottomChanges = segment.changes.slice(-bottom);
            const remainingCount = segment.changes.length - totalShown;
            
            return (
              <React.Fragment key={`segment-${segmentIndex}`}>
                {top > 0 && (
                  <Hunk 
                    hunk={{
                      ...hunk,
                      changes: topChanges,
                      content: hunk.content
                    }}
                  />
                )}
                
                {remainingCount > 0 && (
                  <CollapsedSegment
                    segment={{ ...segment, lineCount: remainingCount }}
                    onExpand={() => handleExpand(segmentIndex)}
                    onExpandUp={() => handleExpandUp(segmentIndex)}
                    onExpandDown={() => handleExpandDown(segmentIndex)}
                    viewType={viewType}
                  />
                )}
                
                {bottom > 0 && (
                  <Hunk 
                    hunk={{
                      ...hunk,
                      changes: bottomChanges,
                      content: hunk.content
                    }}
                  />
                )}
              </React.Fragment>
            );
          }
          
          // Show collapsed segment with expand buttons
          return (
            <CollapsedSegment
              key={`segment-${segmentIndex}`}
              segment={segment}
              onExpand={() => handleExpand(segmentIndex)}
              onExpandUp={() => handleExpandUp(segmentIndex)}
              onExpandDown={() => handleExpandDown(segmentIndex)}
              viewType={viewType}
            />
          );
        }
        
        // Render visible segment as a mini-hunk
        const miniHunk = {
          ...hunk,
          changes: segment.changes,
          content: hunk.content // Keep original hunk header
        };
        
        return (
          <Hunk 
            key={`segment-${segmentIndex}`} 
            hunk={miniHunk}
          />
        );
      })}
    </>
  );
}

/**
 * RawTextViewer - Displays raw diff text as a fallback
 */
function RawTextViewer({ diffText, fileName }) {
  return (
    <div className="diff-raw-text">
      <div className="diff-raw-text-header">
        <span className="diff-raw-text-label">Raw diff text for {fileName}</span>
      </div>
      <pre className="diff-raw-text-content">
        <code>{diffText}</code>
      </pre>
    </div>
  );
}

/**
 * FileDiffError - Displays error message with option to view raw text
 */
function FileDiffError({ fileName, error, diffText, onShowRaw }) {
  const [showingRaw, setShowingRaw] = useState(false);
  
  const handleShowRaw = () => {
    setShowingRaw(true);
    if (onShowRaw) {
      onShowRaw();
    }
  };
  
  if (showingRaw && diffText) {
    return <RawTextViewer diffText={diffText} fileName={fileName} />;
  }
  
  return (
    <div className="diff-file-error">
      <div className="diff-error-icon">⚠️</div>
      <div className="diff-error-content">
        <h4 className="diff-error-title">Failed to parse diff for {fileName}</h4>
        <p className="diff-error-message">
          {error || 'An unexpected error occurred while parsing the diff.'}
        </p>
        {diffText && (
          <button 
            className="diff-error-button"
            onClick={handleShowRaw}
            type="button"
          >
            View raw diff text
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * LoadingState - Displays loading animation
 */
function LoadingState() {
  return (
    <div className="diff-viewer-loading">
      <div className="diff-loading-spinner">
        <svg 
          className="diff-spinner-icon" 
          width="40" 
          height="40" 
          viewBox="0 0 40 40"
          xmlns="http://www.w3.org/2000/svg"
        >
          <circle 
            className="diff-spinner-track"
            cx="20" 
            cy="20" 
            r="17.5" 
            fill="none" 
            strokeWidth="3"
          />
          <circle 
            className="diff-spinner-head"
            cx="20" 
            cy="20" 
            r="17.5" 
            fill="none" 
            strokeWidth="3"
            strokeLinecap="round"
          />
        </svg>
      </div>
      <p className="diff-loading-text">Loading diff...</p>
    </div>
  );
}

/**
 * EmptyState - Displays message when there are no diffs
 */
function EmptyState() {
  return (
    <div className="diff-viewer-empty">
      <div className="diff-empty-icon">
        <svg 
          width="48" 
          height="48" 
          viewBox="0 0 48 48" 
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path 
            d="M24 4C12.96 4 4 12.96 4 24s8.96 20 20 20 20-8.96 20-20S35.04 4 24 4zm0 36c-8.84 0-16-7.16-16-16S15.16 8 24 8s16 7.16 16 16-7.16 16-16 16z" 
            fill="currentColor"
            opacity="0.3"
          />
          <path 
            d="M22 22h4v12h-4V22zm0-8h4v4h-4v-4z" 
            fill="currentColor"
            opacity="0.5"
          />
        </svg>
      </div>
      <h4 className="diff-empty-title">No changes</h4>
      <p className="diff-empty-message">
        There are no file changes between these versions.
      </p>
    </div>
  );
}

/**
 * DiffViewer Component
 * 
 * Renders a list of file diffs with GitHub-style formatting.
 * Includes comprehensive error handling and graceful degradation.
 * 
 * @param {Object} props
 * @param {Array<Object>} props.files - Array of file diff objects from backend API
 * @param {string} props.viewType - View mode: 'split' or 'unified' (default: 'split')
 * @param {boolean} props.enableSyntaxHighlight - Enable syntax highlighting (default: true)
 * @param {number} props.maxLines - Max lines before disabling highlight (default: 10000)
 * @param {boolean} props.loading - Whether the diff is currently loading (default: false)
 * 
 * File object structure (from backend):
 * {
 *   file_path: string,
 *   old_path?: string,
 *   change_type: 'add' | 'delete' | 'modify' | 'rename',
 *   diff_text: string,  // Unified diff format
 *   parsedDiff?: object  // Pre-parsed diff (optional)
 * }
 */
function DiffViewer({ 
  files = [], 
  viewType = 'split', 
  enableSyntaxHighlight = true,
  maxLines = 10000,
  loading = false
}) {
  // Show loading state
  if (loading) {
    return <LoadingState />;
  }

  // Validate props
  if (!Array.isArray(files)) {
    console.error('DiffViewer: files prop must be an array, received:', typeof files);
    return (
      <div className="diff-viewer-error">
        <div className="diff-error-icon">⚠️</div>
        <div className="diff-error-content">
          <h4 className="diff-error-title">Invalid diff data format</h4>
          <p className="diff-error-message">
            Expected an array of file diffs, but received {typeof files}.
          </p>
        </div>
      </div>
    );
  }

  // Handle empty files - show empty state
  if (files.length === 0) {
    return <EmptyState />;
  }

  return (
    <div className="diff-viewer">
      {files.map((fileData, index) => {
        try {
          // Extract data from backend format
          const { file_path, old_path, change_type, parsedDiff, diff_text } = fileData;
          
          // Validate file data
          if (!file_path) {
            console.error('DiffViewer: Missing file_path in file data at index', index);
            return (
              <FileDiffError
                key={index}
                fileName="unknown file"
                error="Missing file path in diff data"
                diffText={diff_text}
              />
            );
          }
          
          // Use pre-parsed diff if available
          const file = parsedDiff;
          
          if (!file) {
            console.error('DiffViewer: Missing parsedDiff for file', file_path);
            return (
              <FileDiffError
                key={index}
                fileName={file_path}
                error="Failed to parse diff. The diff format may be invalid or unsupported."
                diffText={diff_text}
              />
            );
          }
          
          // Validate parsed diff structure
          if (!file.hunks || !Array.isArray(file.hunks)) {
            console.error('DiffViewer: Invalid parsedDiff structure for file', file_path, 'missing hunks array');
            return (
              <FileDiffError
                key={index}
                fileName={file_path}
                error="Invalid diff structure. Missing or invalid hunks data."
                diffText={diff_text}
              />
            );
          }

          // Render the diff
          return (
            <div key={index} className="diff-file">
              <FileHeader 
                file={file} 
                changeType={change_type}
                oldPath={old_path}
              />
              
              <Diff 
                viewType={viewType} 
                diffType={file.type}
                hunks={file.hunks}
                className="diff-content"
              >
                {(hunks) => 
                  hunks.map((hunk, hunkIndex) => (
                    <CollapsibleHunk
                      key={`${index}-hunk-${hunkIndex}`}
                      hunk={hunk}
                      fileIndex={index}
                      hunkIndex={hunkIndex}
                      viewType={viewType}
                    />
                  ))
                }
              </Diff>
            </div>
          );
        } catch (error) {
          // Catch any unexpected errors during rendering
          console.error('DiffViewer: Unexpected error rendering file at index', index, error);
          
          const fileName = fileData?.file_path || `file-${index}`;
          const diffText = fileData?.diff_text;
          
          return (
            <FileDiffError
              key={index}
              fileName={fileName}
              error={`Unexpected error: ${error.message || 'Unknown error'}`}
              diffText={diffText}
            />
          );
        }
      })}
    </div>
  );
}

export default DiffViewer;
