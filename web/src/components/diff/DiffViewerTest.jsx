/**
 * Test component to verify diff viewer dependencies and states
 * This file demonstrates loading, empty, and normal states
 */
import React, { useState } from 'react';
import { parseDiff, Diff, Hunk } from 'react-diff-view';
import { createTwoFilesPatch } from 'diff';
import DiffViewer from './DiffViewer';
import 'react-diff-view/style/index.css';

/**
 * Test component to verify imports work and demonstrate different states
 */
const DiffViewerTest = () => {
  const [currentView, setCurrentView] = useState('normal');
  
  // Create a simple diff for testing
  const oldStr = 'line 1\nline 2\nline 3';
  const newStr = 'line 1\nline 2 modified\nline 3\nline 4';
  
  const diffText = createTwoFilesPatch(
    'test.txt',
    'test.txt',
    oldStr,
    newStr,
    'Old Version',
    'New Version'
  );
  
  const files = parseDiff(diffText);
  
  // Create mock data for DiffViewer
  const mockFiles = [
    {
      file_path: 'test.txt',
      change_type: 'modify',
      diff_text: diffText,
      parsedDiff: files[0]
    }
  ];
  
  return (
    <div style={{ padding: '20px', fontFamily: 'sans-serif' }}>
      <h2>Diff Viewer States Test</h2>
      <p>Select a state to view:</p>
      
      <div style={{ marginBottom: '20px', display: 'flex', gap: '10px' }}>
        <button 
          onClick={() => setCurrentView('normal')}
          style={{
            padding: '8px 16px',
            backgroundColor: currentView === 'normal' ? '#0969da' : '#f6f8fa',
            color: currentView === 'normal' ? 'white' : '#24292f',
            border: '1px solid #d0d7de',
            borderRadius: '6px',
            cursor: 'pointer'
          }}
        >
          Normal State
        </button>
        <button 
          onClick={() => setCurrentView('loading')}
          style={{
            padding: '8px 16px',
            backgroundColor: currentView === 'loading' ? '#0969da' : '#f6f8fa',
            color: currentView === 'loading' ? 'white' : '#24292f',
            border: '1px solid #d0d7de',
            borderRadius: '6px',
            cursor: 'pointer'
          }}
        >
          Loading State
        </button>
        <button 
          onClick={() => setCurrentView('empty')}
          style={{
            padding: '8px 16px',
            backgroundColor: currentView === 'empty' ? '#0969da' : '#f6f8fa',
            color: currentView === 'empty' ? 'white' : '#24292f',
            border: '1px solid #d0d7de',
            borderRadius: '6px',
            cursor: 'pointer'
          }}
        >
          Empty State
        </button>
      </div>
      
      <div style={{ border: '1px solid #d0d7de', borderRadius: '6px', padding: '20px', backgroundColor: '#ffffff' }}>
        {currentView === 'normal' && (
          <>
            <h3>Normal State - With Diff Data</h3>
            <DiffViewer files={mockFiles} viewType="split" />
          </>
        )}
        
        {currentView === 'loading' && (
          <>
            <h3>Loading State</h3>
            <DiffViewer files={[]} loading={true} />
          </>
        )}
        
        {currentView === 'empty' && (
          <>
            <h3>Empty State - No Changes</h3>
            <DiffViewer files={[]} loading={false} />
          </>
        )}
      </div>
      
      <div style={{ marginTop: '40px', padding: '20px', backgroundColor: '#f6f8fa', borderRadius: '6px' }}>
        <h3>Raw react-diff-view Test</h3>
        <p>Basic test to verify dependencies are working:</p>
        {files.map((file, index) => (
          <Diff key={index} viewType="split" diffType={file.type} hunks={file.hunks}>
            {(hunks) => hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)}
          </Diff>
        ))}
      </div>
    </div>
  );
};

export default DiffViewerTest;
