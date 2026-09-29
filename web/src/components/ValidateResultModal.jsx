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
 *     subjectLabel?: string,
 *     subjectValue?: string,
 *     valid: bool,
 *     message: string?,
 *     details: string[],
 *     type: string?  // 配置类型标签，如 "发布包"、"WPL" 等
 *   }
 */
export default function ValidateResultModal({ open, onClose, result }) {
  const { t } = useTranslation();

  if (!result) return null;

  const { filename, subjectLabel, subjectValue, valid, message, details, type } = result;
  const errorMessage = message || (details && details.length > 0 ? details.join('\n') : '');
  const checkResults = analyzeCheckResults(valid, errorMessage);

  const hasError = !valid;
  const statusIcon = hasError ? '✗' : '✓';
  const statusText = hasError ? t('validation.failed') : t('validation.success');
  const typeLabel = type || '';
  const primaryLabel = subjectLabel || t('validation.fileName');
  const primaryValue = subjectValue || filename || '—';

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
      <div className="validate-result-layout">
        <div
          className={`validate-result-summary ${
            hasError ? 'validate-result-summary--error' : 'validate-result-summary--success'
          }`}
        >
          <span className="validate-result-summary-icon">{statusIcon}</span>
          <div className="validate-result-summary-copy">
            <div className="validate-result-summary-title">{statusText}</div>
            {typeLabel ? (
              <div className="validate-result-summary-subtitle">
                {t('validation.conforms', { type: typeLabel })}
              </div>
            ) : null}
          </div>
          </div>

        <div className="validate-result-grid">
          <div className="validate-result-item">
            <div className="validate-result-item-label">{primaryLabel}</div>
            <div className="validate-result-item-value">{primaryValue}</div>
          </div>
          <div className="validate-result-item">
            <div className="validate-result-item-label">{t('validation.validationTime')}</div>
            <div className="validate-result-item-value">
              {new Date().toLocaleString('zh-CN')}
            </div>
          </div>
          <div className="validate-result-item">
            <div className="validate-result-item-label">{t('validation.syntaxCheck')}</div>
            <div
              className={`validate-result-item-value validate-result-item-value--status ${
                checkResults.syntax === 'fail'
                  ? 'is-fail'
                  : checkResults.syntax === 'pass'
                    ? 'is-pass'
                    : 'is-idle'
              }`}
            >
              {syntaxInfo.text}
            </div>
          </div>
          <div className="validate-result-item">
            <div className="validate-result-item-label">{t('validation.formatCheck')}</div>
            <div
              className={`validate-result-item-value validate-result-item-value--status ${
                checkResults.format === 'fail'
                  ? 'is-fail'
                  : checkResults.format === 'pass'
                    ? 'is-pass'
                    : 'is-idle'
              }`}
            >
              {formatInfo.text}
            </div>
          </div>
        </div>

        {hasError && errorMessage && (
          <div className="validate-result-error">
            <div className="validate-result-error-title">{t('validation.errorDetail')}</div>
            <pre className="validate-result-error-content">
              {errorMessage}
            </pre>
          </div>
        )}
      </div>
    </Modal>
  );
}
