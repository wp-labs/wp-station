import React, { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate, useParams } from 'react-router-dom';
import { Modal, Pagination, Select } from 'antd';
import { fetchReleaseDetail, fetchReleaseDiff, rollbackRelease } from '@/services/release';
import DiffViewer from '@/components/diff/DiffViewer';
import { parseDiffText } from '@/components/diff/diffUtils';

const DEFAULT_DIFF_PAGE_SIZE = 10;
const DIFF_PAGE_SIZE_OPTIONS = [5, 10, 20];

/**
 * Adapt old diff data format to new DiffViewer format
 * Converts from { file, current, previous } to unified diff format
 * Includes error handling for parsing failures
 * 
 * @param {Array} oldDiffData - Array of old format diff objects
 * @returns {Array} Array of new format diff objects with parsedDiff
 */
function adaptDiffData(oldDiffData) {
  if (!Array.isArray(oldDiffData)) {
    console.error('adaptDiffData: Expected array, received:', typeof oldDiffData);
    return [];
  }

  return oldDiffData.map((diffItem, index) => {
    try {
      // Check if this is already in the new format (has diff_text)
      if (diffItem.diff_text) {
        // New format from backend - parse the unified diff
        const parsedFiles = parseDiffText(diffItem.diff_text);
        
        // Check if parsing was successful
        if (!parsedFiles || parsedFiles.length === 0) {
          console.warn('adaptDiffData: Failed to parse diff_text for', diffItem.file_path);
          return {
            file_path: diffItem.file_path,
            old_path: diffItem.old_path,
            change_type: diffItem.change_type || 'modify',
            diff_text: diffItem.diff_text,
            parsedDiff: null // Will trigger error display in DiffViewer
          };
        }
        
        return {
          file_path: diffItem.file_path,
          old_path: diffItem.old_path,
          change_type: diffItem.change_type || 'modify',
          diff_text: diffItem.diff_text,
          parsedDiff: parsedFiles[0] || null
        };
      }

      // Old format - need to generate unified diff from current/previous
      const fileName = diffItem.file || 'unknown';
      const currentContent = diffItem.current || '';
      const previousContent = diffItem.previous || '';
      
      // Generate a simple unified diff format
      const diffText = generateSimpleDiff(previousContent, currentContent, fileName);
      const parsedFiles = parseDiffText(diffText);
      
      if (!parsedFiles || parsedFiles.length === 0) {
        console.warn('adaptDiffData: Failed to parse generated diff for', fileName);
        return {
          file_path: fileName,
          old_path: null,
          change_type: determineChangeType(previousContent, currentContent),
          diff_text: diffText,
          parsedDiff: null
        };
      }
      
      return {
        file_path: fileName,
        old_path: null,
        change_type: determineChangeType(previousContent, currentContent),
        diff_text: diffText,
        parsedDiff: parsedFiles[0] || null
      };
    } catch (error) {
      console.error('adaptDiffData: Error processing diff item at index', index, error);
      
      // Return a structure that will show an error in the UI
      const fileName = diffItem?.file_path || diffItem?.file || `file-${index}`;
      return {
        file_path: fileName,
        old_path: diffItem?.old_path || null,
        change_type: diffItem?.change_type || 'modify',
        diff_text: diffItem?.diff_text || null,
        parsedDiff: null,
        error: error.message
      };
    }
  }).filter(item => item !== null); // Keep all items, including those with errors
}

/**
 * Determine change type based on content
 */
function determineChangeType(oldContent, newContent) {
  if (!oldContent || oldContent.trim() === '') {
    return 'add';
  }
  if (!newContent || newContent.trim() === '') {
    return 'delete';
  }
  return 'modify';
}

/**
 * Generate a simple unified diff format from two text contents
 * This is a basic implementation for compatibility with old data format
 */
function generateSimpleDiff(oldContent, newContent, fileName) {
  const oldLines = oldContent.split(/\r?\n/);
  const newLines = newContent.split(/\r?\n/);
  
  // Build unified diff format
  let diff = `--- a/${fileName}\n`;
  diff += `+++ b/${fileName}\n`;
  
  // Simple line-by-line comparison
  const maxLines = Math.max(oldLines.length, newLines.length);
  let hunkStart = 1;
  let hunkLines = [];
  
  for (let i = 0; i < maxLines; i++) {
    const oldLine = oldLines[i];
    const newLine = newLines[i];
    
    if (oldLine === newLine) {
      // Unchanged line
      if (oldLine !== undefined) {
        hunkLines.push(` ${oldLine}`);
      }
    } else {
      // Changed line
      if (oldLine !== undefined) {
        hunkLines.push(`-${oldLine}`);
      }
      if (newLine !== undefined) {
        hunkLines.push(`+${newLine}`);
      }
    }
  }
  
  // Add hunk header
  if (hunkLines.length > 0) {
    diff += `@@ -${hunkStart},${oldLines.length} +${hunkStart},${newLines.length} @@\n`;
    diff += hunkLines.join('\n');
  }
  
  return diff;
}


/**
 * 系统发布详情页面
 * 功能：
 * 1. 显示发布版本详细信息
 * 2. 按机器（device）展示阶段状态
 * 3. 显示配置变更对比
 * 对应原型：pages/views/system-release/release-detail.html
 */
function ReleaseDetailPage() {
  const { t } = useTranslation();
  const { id: releaseId } = useParams();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState(null);
  const [diffData, setDiffData] = useState(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [diffPage, setDiffPage] = useState(1);
  const [diffPageSize, setDiffPageSize] = useState(DEFAULT_DIFF_PAGE_SIZE);
  const [error, setError] = useState(null);

  /**
   * 翻译阶段名称
   */
  const translateStageLabel = (label) => {
    const stageMap = {
      '校验': t('systemRelease.stageValidate'),
      '同步git': t('systemRelease.stageSyncGit'),
      '发布': t('systemRelease.stagePublish'),
      '草稿': t('systemRelease.stageDraft'),
    };
    return stageMap[label] || label;
  };

  /**
   * 加载发布详情数据
   */
  const loadDetail = async () => {
    setLoading(true);
    setError(null);
    try {
      // 调用服务层获取详情（注意：参数名已更改）
      const response = await fetchReleaseDetail(releaseId);
      setDetail(response);
      
      // 加载 diff 数据
      loadDiff();
    } catch (err) {
      console.error('Failed to load release detail:', err);
      setError({
        message: err.message || 'Failed to load release details',
        canRetry: true
      });
    } finally {
      setLoading(false);
    }
  };

  /**
   * 加载版本差异数据
   */
  const loadDiff = async () => {
    setDiffLoading(true);
    try {
      const response = await fetchReleaseDiff(releaseId);
      setDiffData(response?.files || []);
      setDiffPage(1);
    } catch (err) {
      console.error('Failed to load release diff:', err);
      setDiffData([]);
    } finally {
      setDiffLoading(false);
    }
  };

  /**
   * 处理单个设备回滚
   * @param {number} deviceId - 设备 ID
   */
  const handleDeviceRollback = async (deviceId) => {
    if (!window.confirm('确认回滚该设备到上一个成功版本？\n\n回滚后将自动发布到该设备。')) {
      return;
    }

    try {
      setLoading(true);
      const result = await rollbackRelease(releaseId, [deviceId]);
      Modal.success({
        title: '回滚成功',
        content: result.message || '已触发设备回滚，请稍后查看状态',
      });
      setTimeout(() => {
        loadDetail(); // 重新加载详情
      }, 1000);
    } catch (error) {
      Modal.error({
        title: '回滚失败',
        content: error.message || '回滚请求失败，请重试',
      });
    } finally {
      setLoading(false);
    }
  };

  // 组件挂载时加载数据
  useEffect(() => {
    loadDetail();
  }, [releaseId]);

  const adaptedDiffFiles = useMemo(
    () => (Array.isArray(diffData) ? adaptDiffData(diffData) : []),
    [diffData],
  );
  const totalDiffItems = adaptedDiffFiles.length;
  const pagedDiffFiles = useMemo(() => {
    if (!totalDiffItems) {
      return [];
    }
    const start = (diffPage - 1) * diffPageSize;
    return adaptedDiffFiles.slice(start, start + diffPageSize);
  }, [adaptedDiffFiles, diffPage, diffPageSize, totalDiffItems]);

  useEffect(() => {
    const totalPages = Math.max(1, Math.ceil(Math.max(totalDiffItems, 0) / diffPageSize));
    if (diffPage > totalPages) {
      setDiffPage(totalPages);
    }
  }, [totalDiffItems, diffPageSize, diffPage]);

  // 调试：打印 diffData 数据
  useEffect(() => {
    if (diffData) {
      console.log('Release diff data:', {
        diffCount: diffData.length,
        diff: diffData,
        adaptedDiff: adaptedDiffFiles,
      });
    }
  }, [diffData, adaptedDiffFiles]);

  // 加载中状态
  if (loading) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>{t('common.loading')}</div>
      </div>
    );
  }

  // 错误状态
  if (error) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>
          <div style={{ 
            backgroundColor: '#fff5f5', 
            border: '1px solid #feb2b2', 
            borderRadius: '6px',
            padding: '30px',
            maxWidth: '600px',
            margin: '0 auto'
          }}>
            <div style={{ fontSize: '24px', marginBottom: '15px' }}>⚠️</div>
            <h3 style={{ color: '#c53030', marginBottom: '10px' }}>{t('systemRelease.publishFailed')}</h3>
            <p style={{ color: '#742a2a', marginBottom: '20px' }}>{error.message}</p>
            {error.canRetry && (
              <button 
                type="button" 
                className="btn"
                onClick={loadDetail}
                style={{ marginRight: '10px' }}
              >
                {t('systemRelease.query')}
              </button>
            )}
            <button 
              type="button" 
              className="btn ghost"
              onClick={() => navigate('/system-release')}
            >
              {t('systemRelease.backToList')}
            </button>
          </div>
        </div>
      </div>
    );
  }

  // 数据未加载
  if (!detail) {
    return (
      <div className="panel is-visible">
        <div style={{ padding: '40px', textAlign: 'center' }}>{t('common.noData')}</div>
      </div>
    );
  }

  // 根据状态设置类名和文本（与旧版本一致）
  const normalizedStatus = (detail.status || '').toLowerCase();
  const statusTextMap = {
    wait: '草稿',
    pass: '已发布',
    fail: 'FAIL',
    running: 'RUNNING',
    partial_fail: 'PARTIAL_FAIL',
    init: 'INIT',
  };
  const statusText = statusTextMap[normalizedStatus] || (normalizedStatus ? normalizedStatus.toUpperCase() : '—');
  const isPublished = normalizedStatus === 'pass';

  // 状态样式类
  const getStatusClass = (status) => {
    const normalized = (status || '').toLowerCase();
    if (normalized === 'pass') return 'is-pass';
    if (normalized === 'fail') return 'is-fail';
    if (normalized === 'running' || normalized === 'partial_fail') return 'is-running';
    return 'is-wait';
  };

  return (
    <div className="panel is-visible">
      <div className="release-detail">
        {/* 页面头部 */}
        <header className="release-detail-header">
          <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
            <button type="button" className="btn ghost" onClick={() => navigate('/system-release')}>
              {t('systemRelease.backToList')}
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => navigate(`/system-release/${releaseId}/prepublish`)}
            >
              {t(isPublished ? 'sandbox.prepublishDetail' : 'sandbox.startSandbox')}
            </button>
          </div>
          <h3 id="detail-title">{t('systemRelease.versionDetail', { version: detail.version })}</h3>
        </header>

        {/* 发布摘要信息 */}
        <div className="release-summary">
          <div className="summary-item">
            <span className="summary-label">status</span>
            <span className={`summary-value ${getStatusClass(normalizedStatus)}`}>
              {statusText}
            </span>
          </div>
          <div className="summary-item">
            <span className="summary-label">version</span>
            <span className="summary-value" id="detail-version">{detail.version || '—'}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">pipeline</span>
            <span className="summary-value" id="detail-pipeline">{detail.pipeline || '—'}</span>
          </div>
          <div className="summary-item">
            <span className="summary-label">created by</span>
            <span className="summary-value" id="detail-owner">{detail.created_by || detail.owner || '—'}</span>
          </div>
        </div>

        {/* 各机器发布情况 */}
        {Array.isArray(detail.devices) && detail.devices.length > 0 && (
          <div className="release-devices" style={{ margin: '20px 0' }}>
            <header className="release-diff-header">
              <h4>{t('systemRelease.devicesTitle')}</h4>
            </header>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', marginTop: '12px' }}>
              {detail.devices.map((device) => {
                const deviceStatus = String(device.status || '').toUpperCase();
                const deviceStatusTextMap = {
                  'PASS': 'PASS',
                  'FAIL': 'FAIL',
                  'SUCCESS': 'SUCCESS',
                  'RUNNING': 'RUNNING',
                  'QUEUED': 'QUEUED',
                  'ROLLBACKING': 'ROLLBACKING',
                  'ROLLBACK_PENDING': 'ROLLBACK_PENDING',
                  'ROLLED_BACK': 'ROLLED_BACK',
                };
                const deviceStatusText = deviceStatusTextMap[deviceStatus] || deviceStatus || '—';
                const deviceStatusClass = (() => {
                  if (deviceStatus === 'PASS' || deviceStatus === 'SUCCESS') return 'is-pass';
                  if (deviceStatus === 'FAIL') return 'is-fail';
                  if (deviceStatus === 'RUNNING' || deviceStatus === 'QUEUED' || deviceStatus === 'ROLLBACKING') return 'is-running';
                  return 'is-wait';
                })();
                const machineLabel = device.connection_name
                  ? `${device.connection_name} (${device.connection_ip})`
                  : device.connection_ip || `Machine #${device.id}`;

                return (
                  <div
                    key={device.id}
                    style={{
                      border: '1px solid #f0f0f0',
                      borderRadius: '10px',
                      padding: '12px 16px',
                      background: '#fafafa',
                    }}
                  >
                    {/* 机器标题行 */}
                    <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '10px' }}>
                      <span style={{ fontWeight: 600, fontSize: '14px' }}>{machineLabel}</span>
                      <span className={`release-status ${deviceStatusClass}`}>
                        {deviceStatusText}
                      </span>
                      {/* 回滚按钮：FAIL 或 SUCCESS/PASS 状态时显示 */}
                      {(deviceStatus === 'FAIL' || deviceStatus === 'SUCCESS' || deviceStatus === 'PASS') && (
                        <button
                          type="button"
                          className="btn ghost"
                          style={{ marginLeft: 'auto', fontSize: '12px', padding: '4px 12px' }}
                          onClick={() => handleDeviceRollback(device.device_id)}
                        >
                          回滚
                        </button>
                      )}
                    </div>

                    {/* 阶段 chips */}
                    {Array.isArray(device.stages) && device.stages.length > 0 && (
                      <div className="summary-stages">
                        {(() => {
                          let hasFailedBefore = false;
                          return device.stages.map((stage, index) => {
                            const stageStatus = String(stage?.status || '').toLowerCase();
                            let iconClass = 'stage-icon';
                            let iconChar = '>>';

                            if (hasFailedBefore) {
                              iconClass = 'stage-icon';
                              iconChar = '>>';
                            } else if (stageStatus === 'pass') {
                              iconClass = 'stage-icon is-pass';
                              iconChar = '✓';
                            } else if (stageStatus === 'fail') {
                              iconClass = 'stage-icon is-fail';
                              iconChar = '✗';
                              hasFailedBefore = true;
                            } else {
                              iconClass = 'stage-icon';
                              iconChar = '>>';
                            }

                            return (
                              <span key={index} className="stage-chip">
                                <span className={iconClass} title={translateStageLabel(stage.label || '')}>
                                  {iconChar}
                                </span>
                                <span className="stage-name">{translateStageLabel(stage.label || '')}</span>
                              </span>
                            );
                          });
                        })()}
                      </div>
                    )}

                    {/* 失败信息 */}
                    {device.error_message && (
                      <div style={{ marginTop: '8px', fontSize: '13px', color: '#f1554c' }}>
                        {(() => {
                          try {
                            // 尝试提取并格式化 JSON
                            const jsonMatch = device.error_message.match(/\{.*\}/);
                            if (jsonMatch) {
                              const jsonStr = jsonMatch[0];
                              const parsed = JSON.parse(jsonStr);
                              const prefix = device.error_message.substring(0, jsonMatch.index).trim();

                              return (
                                <>
                                  {prefix && <div style={{ marginBottom: '4px' }}>{prefix}</div>}
                                  <pre style={{
                                    background: '#fff1f0',
                                    border: '1px solid #ffccc7',
                                    borderRadius: '4px',
                                    padding: '8px',
                                    margin: 0,
                                    fontSize: '12px',
                                    lineHeight: '1.5',
                                    overflow: 'auto',
                                    maxHeight: '200px'
                                  }}>
                                    {JSON.stringify(parsed, null, 2)}
                                  </pre>
                                </>
                              );
                            }
                          } catch (e) {
                            // JSON 解析失败，显示原始文本
                          }
                          return device.error_message;
                        })()}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* 失败信息（全局，仅在无 devices 时显示旧格式） */}
        {detail.errorMessage && (!Array.isArray(detail.devices) || detail.devices.length === 0) && (
          <div className="release-error" id="release-error">
            <header className="release-diff-header">
              <h4>{t('systemRelease.publishFailed')}</h4>
            </header>
            <div className="release-error-content" id="release-error-content">
              {detail.errorMessage}
            </div>
          </div>
        )}

        {/* 版本差异 */}
        <div className="release-diff">
          <header
            className="release-diff-header"
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              flexWrap: 'wrap',
              gap: 12,
            }}
          >
            <div>
              <h4 style={{ marginBottom: 4 }}>{t('systemRelease.versionDiff')}</h4>
              <span className="release-diff-hint">
                {t('systemRelease.currentVersion')} vs {t('systemRelease.previousVersion')}
              </span>
            </div>
            {totalDiffItems > 0 ? (
              <div
                className="release-diff-controls"
                style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}
              >
                <span style={{ fontSize: 12, color: '#666' }}>
                  {t('systemRelease.diffPageSizeLabel')}
                </span>
                <Select
                  size="small"
                  value={diffPageSize}
                  style={{ width: 140 }}
                  options={DIFF_PAGE_SIZE_OPTIONS.map((size) => ({
                    value: size,
                    label: `${size} ${t('systemRelease.diffPageSizeUnit')}`,
                  }))}
                  onChange={(value) => {
                    setDiffPageSize(value);
                    setDiffPage(1);
                  }}
                />
                {totalDiffItems > diffPageSize ? (
                  <Pagination
                    size="small"
                    simple
                    pageSize={diffPageSize}
                    current={diffPage}
                    total={totalDiffItems}
                    onChange={(page) => setDiffPage(page)}
                  />
                ) : null}
              </div>
            ) : null}
          </header>
          <div className="release-diff-grid" id="release-diff">
            <DiffViewer
              files={pagedDiffFiles}
              viewType="split"
              loading={diffLoading}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export default ReleaseDetailPage;
