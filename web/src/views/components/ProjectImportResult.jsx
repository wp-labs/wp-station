import React from 'react';
import { useTranslation } from 'react-i18next';

function ProjectImportResult({ result, showPaths = true, system = 'wparse' }) {
  const { t } = useTranslation();
  const importSummary = result?.summary;
  const importValidation = result?.validation;
  const normalizedSystem = system === 'wfusion' ? 'wfusion' : 'wparse';
  const importStatCards =
    normalizedSystem === 'wfusion'
      ? [
          {
            key: 'rules',
            label: t('systemManage.initImportRulesLabel'),
            value: importSummary?.rules_imported || 0,
          },
          {
            key: 'rulesDeleted',
            label: t('systemManage.initImportRulesDeletedLabel'),
            value: importSummary?.rules_deleted || 0,
          },
        ]
      : [
          {
            key: 'rules',
            label: t('systemManage.initImportRulesLabel'),
            value: importSummary?.rules_imported || 0,
          },
          {
            key: 'knowledge',
            label: t('systemManage.initImportKnowledgeLabel'),
            value: importSummary?.knowledge_imported || 0,
          },
          {
            key: 'rulesDeleted',
            label: t('systemManage.initImportRulesDeletedLabel'),
            value: importSummary?.rules_deleted || 0,
          },
          {
            key: 'knowledgeDeleted',
            label: t('systemManage.initImportKnowledgeDeletedLabel'),
            value: importSummary?.knowledge_deleted || 0,
          },
        ];
  const importSuccessCount =
    (importSummary?.rules_imported || 0) + (importSummary?.knowledge_imported || 0);
  const importFailureCount = importSummary?.failed_files || 0;
  const breakdownItems = importSummary?.rule_breakdown || [];
  const summaryWarnings = importSummary?.warnings || [];
  const importedDirs = importSummary?.imported_dirs || [];
  const retainedDirs = importSummary?.retained_dirs || [];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
        <div
          style={{
            background: '#ecfdf5',
            border: '1px solid #a7f3d0',
            borderRadius: 10,
            padding: '10px 16px',
            minWidth: 180,
          }}
        >
          <p style={{ margin: 0, color: '#047857', fontSize: 13 }}>
            {t('systemManage.initImportResultSuccess', { count: importSuccessCount })}
          </p>
          <p style={{ margin: 0, fontWeight: 600, color: '#065f46', fontSize: 18 }}>
            {importSuccessCount}
          </p>
        </div>
        <div
          style={{
            background: '#fff7ed',
            border: '1px solid #fed7aa',
            borderRadius: 10,
            padding: '10px 16px',
            minWidth: 180,
          }}
        >
          <p style={{ margin: 0, color: '#c2410c', fontSize: 13 }}>
            {t('systemManage.initImportResultFailure', { count: importFailureCount })}
          </p>
          <p style={{ margin: 0, fontWeight: 600, color: '#b45309', fontSize: 18 }}>
            {importFailureCount}
          </p>
        </div>
      </div>

      <div
        style={{
          background: '#f8fafc',
          borderRadius: 12,
          padding: 16,
          border: '1px solid #e2e8f0',
        }}
      >
        <p style={{ marginBottom: 12, fontWeight: 600 }}>
          {t('systemManage.initImportSummaryTitle')}
        </p>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))',
            gap: 12,
          }}
        >
          {importStatCards.map((card) => (
            <div
              key={card.key}
              style={{
                background: '#fff',
                borderRadius: 10,
                border: '1px solid #e5e7eb',
                padding: '10px 12px',
                boxShadow: '0 1px 2px rgba(15, 23, 42, 0.04)',
              }}
            >
              <p style={{ margin: 0, color: '#64748b', fontSize: 12 }}>{card.label}</p>
              <p style={{ margin: 0, fontSize: 20, fontWeight: 700, color: '#0f172a' }}>
                {card.value}
              </p>
            </div>
          ))}
        </div>

        {breakdownItems.length > 0 && (
          <div style={{ marginTop: 16 }}>
            <p style={{ marginBottom: 8, fontWeight: 500 }}>
              {t('systemManage.initImportRuleBreakdownTitle')}
            </p>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))',
                gap: 8,
              }}
            >
              {breakdownItems.map((item) => (
                <div
                  key={item.rule_type}
                  style={{
                    border: '1px dashed #cbd5f5',
                    borderRadius: 8,
                    padding: '8px 10px',
                    background: '#fff',
                    fontSize: 13,
                    color: '#1e293b',
                  }}
                >
                  <div style={{ fontWeight: 500 }}>
                    {formatRuleTypeLabel(item.rule_type, normalizedSystem)}
                  </div>
                  <div style={{ fontSize: 16, fontWeight: 600 }}>{item.count}</div>
                </div>
              ))}
            </div>
          </div>
        )}

        {(importedDirs.length > 0 || retainedDirs.length > 0) && (
          <div style={{ marginTop: 16, display: 'grid', gap: 12 }}>
            {importedDirs.length > 0 && (
              <DirListBlock
                label={t('systemManage.initImportImportedDirsTitle')}
                values={importedDirs}
                tone="success"
              />
            )}
            {retainedDirs.length > 0 && (
              <DirListBlock
                label={t('systemManage.initImportRetainedDirsTitle')}
                values={retainedDirs}
                tone="warning"
              />
            )}
          </div>
        )}

        {showPaths && normalizedSystem !== 'wfusion' && (
          <div style={{ marginTop: 16, display: 'grid', gap: 12 }}>
            <PathBlock label={t('systemManage.initImportSourceDirLabel')} value={importSummary?.source_dir} />
            <PathBlock label={t('systemManage.initImportProjectModels')} value={importSummary?.models_root} />
            <PathBlock label={t('systemManage.initImportProjectInfra')} value={importSummary?.infra_root} />
          </div>
        )}
      </div>

      {summaryWarnings.length > 0 && (
        <div style={{ background: '#fff7ed', borderRadius: 8, padding: 12 }}>
          <p style={{ marginBottom: 6, color: '#c2410c', fontWeight: 500 }}>
            {t('systemManage.initImportWarnings')}
          </p>
          <ul style={{ paddingLeft: 16, margin: 0, color: '#9a3412', lineHeight: 1.6 }}>
            {summaryWarnings.map((warning, index) => (
              <li key={index}>{warning}</li>
            ))}
          </ul>
        </div>
      )}

      {importValidation && (
        <div
          style={{
            background: importValidation.passed ? '#ecfdf5' : '#fef2f2',
            color: importValidation.passed ? '#047857' : '#b91c1c',
            borderRadius: 8,
            padding: 12,
          }}
        >
          <p style={{ margin: 0, fontWeight: 600 }}>
            {importValidation.passed
              ? t('systemManage.initImportValidationPassed')
              : t('systemManage.initImportValidationFailed')}
          </p>
          <p style={{ margin: '4px 0 0', fontSize: 13 }}>
            {importValidation.message}
          </p>
        </div>
      )}
    </div>
  );
}

function formatRuleTypeLabel(ruleType, system) {
  const normalizedSystem = system === 'wfusion' ? 'wfusion' : 'wparse';
  const value = String(ruleType || '').trim();
  if (!value) return '-';

  if (normalizedSystem === 'wfusion') {
    const labels = {
      parse: 'conf',
      windows: 'windows',
      schema: 'schema',
      rule: 'rule',
      scenarios: 'scenarios',
      source: 'source',
      sink: 'sink',
      source_connect: 'source_connect',
      sink_connect: 'sink_connect',
    };
    return labels[value] || value;
  }

  const labels = {
    parse: 'parse',
    rule: 'rule',
    source: 'source',
    sink: 'sink',
    source_connect: 'source_connect',
    sink_connect: 'sink_connect',
  };
  return labels[value] || value;
}

function PathBlock({ label, value }) {
  return (
    <div>
      <p style={{ margin: 0, color: '#475569', fontWeight: 500 }}>{label}</p>
      <code
        style={{
          display: 'block',
          marginTop: 6,
          padding: '8px 10px',
          background: '#fff',
          borderRadius: 8,
          border: '1px solid #e2e8f0',
          fontFamily: 'monospace',
          color: '#0f172a',
        }}
      >
        {value || '-'}
      </code>
    </div>
  );
}

function DirListBlock({ label, values, tone = 'success' }) {
  const palette =
    tone === 'warning'
      ? { background: '#fff7ed', border: '#fdba74', color: '#9a3412' }
      : { background: '#ecfdf5', border: '#86efac', color: '#166534' };

  return (
    <div>
      <p style={{ margin: 0, color: '#475569', fontWeight: 500 }}>{label}</p>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 8 }}>
        {values.map((value) => (
          <span
            key={value}
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              padding: '4px 10px',
              borderRadius: 999,
              background: palette.background,
              border: `1px solid ${palette.border}`,
              color: palette.color,
              fontSize: 12,
              fontWeight: 500,
            }}
          >
            {value}
          </span>
        ))}
      </div>
    </div>
  );
}

export default ProjectImportResult;
