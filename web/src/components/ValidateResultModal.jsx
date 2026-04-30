import React from 'react';
import { Modal } from 'antd';
import { useTranslation } from 'react-i18next';

/**
 * 根据错误消息分析语法检查和格式检查的结果
 * @param {boolean} valid - 校验是否通过
 * @param {string} errorMessage - 错误消息
 * @returns {{ syntax: 'pass'|'fail'|'not_checked', format: 'pass'|'fail'|'not_checked' }}
 */
function analyzeCheckResults(valid, errorMessage) {
  if (valid) {
    return { syntax: 'pass', format: 'pass' };
  }
  const msg = (errorMessage || '').toLowerCase();
  // TOML parse error / deserialize error → 语法检查失败
  if (msg.includes('toml parse error') || msg.includes('parse error')) {
    return { syntax: 'fail', format: 'not_checked' };
  }
  // 其他错误（如字段校验失败）→ 语法通过，格式/业务校验失败
  return { syntax: 'pass', format: 'fail' };
}

/**
 * 获取校验状态对应的展示信息
 */
function getStatusInfo(checkResult, t) {
  switch (checkResult) {
    case 'pass':
      return { text: t('validation.passed'), color: '#52c41a' };
    case 'fail':
      return { text: t('validation.failed_status'), color: '#ff4d4f' };
    case 'not_checked':
    default:
      return { text: t('validation.notChecked'), color: '#999' };
  }
}

/**
 * 共享校验结果弹窗组件
 *
 * Props:
 *   open: bool - 是否显示
 *   onClose: func - 关闭回调
 *   result: {
 *     filename: string,
 *     valid: bool,
 *     message: string?,
 *     details: string[],
 *     type: string?  // 配置类型标签，如 "发布包"、"WPL" 等
 *   }
 */
export default function ValidateResultModal({ open, onClose, result }) {
  const { t } = useTranslation();

  if (!result) return null;

  const { filename, valid, message, details, type } = result;
  const errorMessage = message || (details && details.length > 0 ? details.join('\n') : '');
  const checkResults = analyzeCheckResults(valid, errorMessage);

  const hasError = !valid;
  const statusColor = hasError ? '#ff4d4f' : '#52c41a';
  const statusIcon = hasError ? '✗' : '✓';
  const statusText = hasError ? t('validation.failed') : t('validation.success');
  const typeLabel = type || '';

  const syntaxInfo = getStatusInfo(checkResults.syntax, t);
  const formatInfo = getStatusInfo(checkResults.format, t);

  return (
    <Modal
      title={t('validation.result')}
      open={open}
      onCancel={onClose}
      footer={[
        <button
          key="confirm"
          type="button"
          className="btn primary"
          onClick={onClose}
        >
          {t('common.confirm')}
        </button>,
      ]}
      width={580}
      className="validate-result-modal"
    >
      <div>
        {/* 状态栏 */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '12px',
            marginBottom: '20px',
            padding: '16px',
            background: hasError ? '#fff2f0' : '#f6ffed',
            borderLeft: `3px solid ${statusColor}`,
            borderRadius: '8px',
          }}
        >
          <span style={{ fontSize: '28px', color: statusColor }}>{statusIcon}</span>
          <div>
            <div style={{ fontSize: '16px', fontWeight: 600, color: statusColor, marginBottom: '4px' }}>
              {statusText}
            </div>
            {typeLabel ? (
              <div style={{ fontSize: '13px', color: '#666' }}>
                {t('validation.conforms', { type: typeLabel })}
              </div>
            ) : null}
          </div>
        </div>

        {/* 详情表格 */}
        <div style={{ background: '#fafafa', borderRadius: '8px', padding: '16px', marginBottom: hasError ? '16px' : '0' }}>
          <table style={{ width: '100%', fontSize: '13px', lineHeight: '2' }}>
            <tbody>
              <tr>
                <td style={{ color: '#666', padding: '4px 0', whiteSpace: 'nowrap', verticalAlign: 'top' }}>
                  {t('validation.fileName')}
                </td>
                <td style={{ fontWeight: 500, padding: '4px 0' }}>{filename || '—'}</td>
              </tr>
              <tr>
                <td style={{ color: '#666', padding: '4px 0', whiteSpace: 'nowrap', verticalAlign: 'top' }}>
                  {t('validation.syntaxCheck')}
                </td>
                <td style={{ fontWeight: 500, color: syntaxInfo.color, padding: '4px 0' }}>
                  {syntaxInfo.text}
                </td>
              </tr>
              <tr>
                <td style={{ color: '#666', padding: '4px 0', whiteSpace: 'nowrap', verticalAlign: 'top' }}>
                  {t('validation.formatCheck')}
                </td>
                <td style={{ fontWeight: 500, color: formatInfo.color, padding: '4px 0' }}>
                  {formatInfo.text}
                </td>
              </tr>
              <tr>
                <td style={{ color: '#666', padding: '4px 0', whiteSpace: 'nowrap', verticalAlign: 'top' }}>
                  {t('validation.validationTime')}
                </td>
                <td style={{ fontWeight: 500, padding: '4px 0' }}>
                  {new Date().toLocaleString('zh-CN')}
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        {/* 错误详情 */}
        {hasError && errorMessage && (
          <div
            style={{
              background: '#fff2f0',
              border: '1px solid #ffccc7',
              borderRadius: '8px',
              padding: '12px 16px',
            }}
          >
            <div style={{ fontSize: '13px', fontWeight: 600, color: '#ff4d4f', marginBottom: '8px' }}>
              {t('validation.errorDetail')}
            </div>
            <pre
              style={{
                margin: 0,
                padding: '12px',
                background: '#fff',
                border: '1px solid #ffccc7',
                borderRadius: '4px',
                fontSize: '12px',
                fontFamily: "'SF Mono', 'Monaco', 'Menlo', 'Consolas', monospace",
                lineHeight: '1.6',
                color: '#333',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
                maxHeight: '320px',
                overflowY: 'auto',
              }}
            >
              {errorMessage}
            </pre>
          </div>
        )}
      </div>
    </Modal>
  );
}
