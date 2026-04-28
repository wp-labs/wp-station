import React, { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input, message, Modal } from 'antd';
import {
  RuleType,
  fetchRuleConfig,
  validateRuleConfig,
  saveRuleConfig,
  fetchRuleFiles,
  fetchConnectionFiles,
  createConnectionConfigFile,
  deleteConnectionConfigFile,
} from '@/services/config';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';
import ValidateResultModal from '@/components/ValidateResultModal';

const CONNECTION_FILE_ORDER = Object.freeze([
  '00-file-default.toml',
  '10-syslog-udp.toml',
  '11-syslog-tcp.toml',
  '12-tcp.toml',
  '30-kafka.toml',
  '40-mysql.toml',
  '00-blackhole-sink.toml',
  '01-file-prototext.toml',
  '02-file-json.toml',
  '03-file-kv.toml',
  '04-file-raw.toml',
  '09-file-test.toml',
  '40-prometheus.toml',
  '50-mysql.toml',
  '60-doris.toml',
  '60-postgres.toml',
  '70-victorialogs.toml',
  '80-victoriametrics.toml',
  '90-elasticsearch.toml',
  '100-clickhouse.toml',
  '101-http.toml',
]);

const sortSinkItems = (items = []) =>
  items
    .map((item) => {
      if (!item?.file) {
        return null;
      }

      return {
        file: item.file,
        displayName:
          typeof item.displayName === 'string' && item.displayName.trim()
            ? item.displayName.trim()
            : item.file,
      };
    })
    .filter(Boolean)
    .sort((a, b) => a.displayName.localeCompare(b.displayName, 'zh-CN'));

function ConfigManagePage() {
  const { t } = useTranslation();
  const [activeKey, setActiveKey] = useState(RuleType.PARSE);
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [sinkFiles, setSinkFiles] = useState([]);
  const [activeSinkFile, setActiveSinkFile] = useState('');
  const [connectionFiles, setConnectionFiles] = useState({ sources: [], sinks: [] });
  const [activeConnectionFile, setActiveConnectionFile] = useState('');
  const [activeConnectionCategory, setActiveConnectionCategory] = useState('source_connect');
  const [hoveredConnectionFile, setHoveredConnectionFile] = useState('');
  const [hoveredConnectionCategory, setHoveredConnectionCategory] = useState('');
  const [connectionSearch, setConnectionSearch] = useState('');
  const [validateModalVisible, setValidateModalVisible] = useState(false);
  const [validateResult, setValidateResult] = useState(null);
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [addModalType, setAddModalType] = useState('source');
  const [newFileName, setNewFileName] = useState('');
  const [newDisplayName, setNewDisplayName] = useState('');
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [originalContent, setOriginalContent] = useState('');

  const getConnectionLabel = React.useCallback(
    (file, displayName) => {
      if (displayName && String(displayName).trim()) {
        return String(displayName).trim();
      }

      const currentItem = [
        ...(connectionFiles.sources || []),
        ...(connectionFiles.sinks || []),
      ].find((item) => item?.file === file);

      return currentItem?.displayName || file || '';
    },
    [connectionFiles.sinks, connectionFiles.sources],
  );

  const sortConnectionItems = React.useCallback(
    (items, category) => {
      const orderMap = new Map(CONNECTION_FILE_ORDER.map((file, index) => [file, index]));

      return (items || [])
        .filter((item) => {
          if (!connectionSearch) {
            return true;
          }

          const keyword = connectionSearch.toLowerCase();
          return (
            (item.displayName || item.file || '').toLowerCase().includes(keyword) ||
            item.file.toLowerCase().includes(keyword)
          );
        })
        .map((item) => ({
          key: `${category}:${item.file}`,
          file: item.file,
          category,
          displayName: item.displayName || item.file,
        }))
        .sort((a, b) => {
          const aOrder = orderMap.has(a.file) ? orderMap.get(a.file) : Number.MAX_SAFE_INTEGER;
          const bOrder = orderMap.has(b.file) ? orderMap.get(b.file) : Number.MAX_SAFE_INTEGER;

          if (aOrder !== bOrder) {
            return aOrder - bOrder;
          }

          return a.displayName.localeCompare(b.displayName, 'zh-CN');
        });
    },
    [connectionSearch],
  );

  const sourceConnectionItems = useMemo(
    () => sortConnectionItems(connectionFiles.sources, 'source_connect'),
    [connectionFiles.sources, sortConnectionItems],
  );
  const sinkConnectionItems = useMemo(
    () => sortConnectionItems(connectionFiles.sinks, 'sink_connect'),
    [connectionFiles.sinks, sortConnectionItems],
  );
  const displayedSinkFiles = useMemo(() => sortSinkItems(sinkFiles), [sinkFiles]);

  const getDefaultFileForKey = (key) => {
    if (key === RuleType.PARSE) {
      return 'wparse.toml';
    }
    if (key === RuleType.SOURCE) {
      return 'wpsrc.toml';
    }
    if (key === RuleType.SINK) {
      return activeSinkFile || '';
    }
    if (key === 'connection') {
      return activeConnectionFile || '';
    }
    return '';
  };

  const getCurrentConfigLabel = () => {
    if (activeKey === RuleType.PARSE) {
      return t('configManage.parseConfig');
    }
    if (activeKey === RuleType.SOURCE) {
      return t('configManage.sourceConfig');
    }
    if (activeKey === RuleType.SINK) {
      return t('configManage.sinkConfig');
    }
    return activeConnectionCategory === 'sink_connect'
      ? t('configManage.sinkConnect')
      : t('configManage.sourceConnect');
  };

  const loadConnectionFiles = async (keyword) => {
    const files = await fetchConnectionFiles({
      keyword: keyword || undefined,
    });
    setConnectionFiles(files);
    return files;
  };

  const loadSinkFiles = async () => {
    const response = await fetchRuleFiles({ type: RuleType.SINK });
    const files = Array.isArray(response?.items) ? response.items : [];
    setSinkFiles(files);
    return files;
  };

  const ensureSelectionAfterListLoad = React.useCallback(
    (key, files) => {
      if (key === RuleType.SINK) {
        const normalized = sortSinkItems(files);
        const activeExists = normalized.some((item) => item.file === activeSinkFile);
        if (!activeExists) {
          setActiveSinkFile(normalized[0]?.file || '');
        }
        return;
      }

      if (key === 'connection') {
        const merged = [
          ...(files.sources || []).map((item) => ({ file: item.file, category: 'source_connect' })),
          ...(files.sinks || []).map((item) => ({ file: item.file, category: 'sink_connect' })),
        ];

        const activeExists = merged.some(
          (item) =>
            item.file === activeConnectionFile && item.category === activeConnectionCategory,
        );

        if (!activeExists) {
          const nextItem = merged[0] || null;
          setActiveConnectionFile(nextItem?.file || '');
          setActiveConnectionCategory(nextItem?.category || 'source_connect');
        }
      }
    },
    [activeConnectionCategory, activeConnectionFile, activeSinkFile],
  );

  const loadConfig = async () => {
    setLoading(true);
    try {
      if (activeKey === RuleType.SINK) {
        if (!activeSinkFile) {
          setContent('');
          setOriginalContent('');
          setHasUnsavedChanges(false);
          return;
        }
        const response = await fetchRuleConfig({ type: RuleType.SINK, file: activeSinkFile });
        const newContent = response?.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
        return;
      }

      if (activeKey === 'connection') {
        if (!activeConnectionFile) {
          setContent('');
          setOriginalContent('');
          setHasUnsavedChanges(false);
          return;
        }

        const type =
          activeConnectionCategory === 'sink_connect'
            ? RuleType.SINK_CONNECT
            : RuleType.SOURCE_CONNECT;
        const response = await fetchRuleConfig({ type, file: activeConnectionFile });
        const newContent = response?.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
        return;
      }

      const response = await fetchRuleConfig({ type: activeKey });
      const newContent = response?.content || '';
      setContent(newContent);
      setOriginalContent(newContent);
      setHasUnsavedChanges(false);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (activeKey === RuleType.SINK) {
      loadSinkFiles()
        .then((files) => ensureSelectionAfterListLoad(activeKey, files))
        .catch((error) => {
          message.error(t('configManage.loadSinkFailed', { message: error.message }));
        });
      return;
    }

    if (activeKey === 'connection') {
      loadConnectionFiles(connectionSearch)
        .then((files) => ensureSelectionAfterListLoad(activeKey, files))
        .catch((error) => {
          message.error(t('configManage.loadConnectionFailed', { message: error.message }));
        });
    }
  }, [activeKey]);

  useEffect(() => {
    loadConfig();
  }, [activeKey, activeSinkFile, activeConnectionFile, activeConnectionCategory]);

  useEffect(() => {
    setHasUnsavedChanges(content !== originalContent);
  }, [content, originalContent]);

  const confirmBeforeSwitch = (onConfirm) => {
    if (!hasUnsavedChanges) {
      onConfirm();
      return;
    }

    Modal.confirm({
      title: t('configManage.leaveConfirm'),
      content: t('configManage.leaveConfirmMessage'),
      okText: t('common.confirm'),
      cancelText: t('common.cancel'),
      onOk: () => {
        setHasUnsavedChanges(false);
        onConfirm();
      },
    });
  };

  const handleNavigation = (newKey) => {
    if (newKey === activeKey) {
      return;
    }

    confirmBeforeSwitch(() => {
      setActiveKey(newKey);
    });
  };

  const handleValidate = async () => {
    try {
      let type = activeKey;
      let file = getDefaultFileForKey(activeKey);

      if (activeKey === 'connection') {
        if (!activeConnectionFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }
        type =
          activeConnectionCategory === 'sink_connect'
            ? RuleType.SINK_CONNECT
            : RuleType.SOURCE_CONNECT;
        file = activeConnectionFile;
      }

      if (activeKey === RuleType.SINK && !activeSinkFile) {
        message.warning(t('configManage.noFileSelected'));
        return;
      }

      const response = await validateRuleConfig({
        type,
        file,
        content: content || '',
      });

      setValidateResult({
        filename: response.filename || file,
        valid: Boolean(response.valid),
        message: response.message || null,
        details: response.details || [],
        type: getCurrentConfigLabel(),
      });
      setValidateModalVisible(true);
    } catch (error) {
      setValidateResult({
        filename: '',
        valid: false,
        message: error.message || '未知错误',
        details: [],
        type: getCurrentConfigLabel(),
      });
      setValidateModalVisible(true);
    }
  };

  const handleSave = async () => {
    try {
      if (activeKey === RuleType.SINK) {
        if (!activeSinkFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }
        await saveRuleConfig({
          type: RuleType.SINK,
          file: activeSinkFile,
          content,
        });
      } else if (activeKey === 'connection') {
        if (!activeConnectionFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }

        const type =
          activeConnectionCategory === 'sink_connect'
            ? RuleType.SINK_CONNECT
            : RuleType.SOURCE_CONNECT;
        await saveRuleConfig({
          type,
          file: activeConnectionFile,
          content,
        });
      } else {
        await saveRuleConfig({
          type: activeKey,
          file: getDefaultFileForKey(activeKey),
          content,
        });
      }

      setOriginalContent(content);
      setHasUnsavedChanges(false);

      Modal.info({
        icon: null,
        okText: t('common.confirm'),
        width: 420,
        title: t('configManage.saveSuccess'),
        content: (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              paddingTop: 4,
              paddingBottom: 4,
            }}
          >
            <span style={{ fontSize: 28, color: '#52c41a' }}>✓</span>
            <div>
              <div
                style={{
                  fontSize: 16,
                  fontWeight: 600,
                  color: '#52c41a',
                  marginBottom: 4,
                }}
              >
                {t('configManage.saveSuccess')}
              </div>
              <div style={{ fontSize: 13, color: '#666' }}>{t('configManage.saveSuccessMessage')}</div>
            </div>
          </div>
        ),
      });
    } catch (error) {
      message.error(`${t('configManage.saveFailed')}：${error.message}`);
    }
  };

  const handleDeleteConnectionFile = async (category, file) =>
    new Promise((resolve, reject) => {
      Modal.confirm({
        title: t('configManage.deleteConfirm'),
        content: t('configManage.deleteConfirmMessage', { filename: file }),
        okText: t('common.delete'),
        okButtonProps: { danger: true },
        cancelText: t('common.cancel'),
        onOk: async () => {
          try {
            await deleteConnectionConfigFile({ category, file });
            const refreshed = await loadConnectionFiles(connectionSearch);
            ensureSelectionAfterListLoad('connection', refreshed);
            message.success(t('configManage.deleteSuccess'));
            resolve(true);
          } catch (error) {
            message.error(t('configManage.deleteFailed', { message: error.message }));
            reject(error);
          }
        },
        onCancel: () => resolve(false),
      });
    });

  const handleCreateConnectionConfigFile = async () => {
    const normalized = newFileName.trim();
    const normalizedDisplayName = newDisplayName.trim();

    if (!normalized) {
      message.warning(t('configManage.enterFileName'));
      return;
    }
    if (!normalizedDisplayName) {
      message.warning(t('configManage.enterDisplayName'));
      return;
    }

    const category = addModalType === 'source' ? 'source_connect' : 'sink_connect';

    try {
      await createConnectionConfigFile({
        category,
        file: normalized,
        displayName: normalizedDisplayName,
      });
      const refreshed = await loadConnectionFiles(connectionSearch);
      setActiveConnectionFile(normalized);
      setActiveConnectionCategory(category);
      ensureSelectionAfterListLoad('connection', refreshed);
      message.success(t('configManage.createSuccess', { filename: normalizedDisplayName }));
      setAddModalVisible(false);
      setNewFileName('');
      setNewDisplayName('');
    } catch (error) {
      message.error(t('configManage.createFailed', { message: error.message }));
    }
  };

  const renderSingleConfig = (fileName, language = 'toml') => (
    <div className="single-config">
      <header className="single-config-header">
        <span className="single-config-name">{fileName}</span>
        <div className="single-config-actions">
          <button type="button" className="btn tertiary" onClick={handleValidate}>
            {t('configManage.validate')}
          </button>
          <button type="button" className="btn primary" onClick={handleSave}>
            {t('configManage.save')}
          </button>
        </div>
      </header>
      <CodeEditor
        className="code-area code-area--full"
        value={content}
        onChange={(value) => setContent(value)}
        language={language}
        theme="vscodeDark"
      />
    </div>
  );

  const renderSinkConfig = () => (
    <div className="repo-layout" data-repo="sink">
      <aside className="repo-tree" aria-label="sink 配置文件列表">
        <h3>{t('configManage.configFiles')}</h3>
        <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
          {displayedSinkFiles.map((item) => (
            <button
              key={item.file}
              type="button"
              className={`repo-file ${activeSinkFile === item.file ? 'is-active' : ''}`}
              onClick={() =>
                confirmBeforeSwitch(() => {
                  setActiveSinkFile(item.file);
                })
              }
              style={{ textAlign: 'left' }}
            >
              {item.displayName}
            </button>
          ))}
        </div>
      </aside>

      <div className="repo-content">
        <div className="repo-toolbar">
          <div className="repo-path">{activeSinkFile || t('configManage.noFileSelected')}</div>
          <div className="editor-actions">
            <button type="button" className="btn tertiary" onClick={handleValidate}>
              {t('configManage.validate')}
            </button>
            <button type="button" className="btn primary" onClick={handleSave}>
              {t('configManage.save')}
            </button>
          </div>
        </div>
        <div className="repo-view">
          <CodeEditor
            className="code-area code-area--large repo-doc is-visible"
            value={content}
            onChange={(value) => setContent(value)}
            language="toml"
            theme="vscodeDark"
          />
        </div>
      </div>
    </div>
  );

  const renderConnectionConfig = () => (
    <div className="repo-layout" data-repo="connection">
      <aside className="repo-tree" aria-label="连接配置文件列表">
        <div className="repo-tree-header">
          <h3>{t('configManage.configFiles')}</h3>
          <button
            type="button"
            className="btn ghost repo-add-btn"
            onClick={() => setAddModalVisible(true)}
          >
            {t('configManage.add')}
          </button>
        </div>
        <div style={{ padding: '4px 0 8px' }}>
          <Input
            size="small"
            allowClear
            placeholder={t('configManage.searchPlaceholder')}
            value={connectionSearch}
            onChange={(e) => {
              const value = e.target.value;
              setConnectionSearch(value);
              loadConnectionFiles(value)
                .then((files) => ensureSelectionAfterListLoad('connection', files))
                .catch((error) => {
                  message.error(t('configManage.loadConnectionFailed', { message: error.message }));
                });
            }}
          />
        </div>
        <div className="repo-folder">
          <div className="repo-folder-header">{t('configManage.source')}</div>
          <div className="repo-folder-content">
            {sourceConnectionItems.map((item) => (
              <div
                key={item.key}
                className="repo-file-row"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 8,
                }}
                onMouseEnter={() => {
                  setHoveredConnectionFile(item.file);
                  setHoveredConnectionCategory(item.category);
                }}
                onMouseLeave={() => {
                  setHoveredConnectionFile('');
                  setHoveredConnectionCategory('');
                }}
              >
                <button
                  type="button"
                  className={`repo-file ${
                    activeConnectionCategory === item.category && activeConnectionFile === item.file
                      ? 'is-active'
                      : ''
                  }`}
                  onClick={() =>
                    confirmBeforeSwitch(() => {
                      setActiveConnectionFile(item.file);
                      setActiveConnectionCategory(item.category);
                    })
                  }
                  style={{ flex: 1, textAlign: 'left' }}
                  title={item.file}
                >
                  {item.displayName}
                </button>
                <button
                  type="button"
                  className="repo-file-delete"
                  style={{
                    minWidth: 20,
                    width: 20,
                    height: 20,
                    borderRadius: '50%',
                    border: 'none',
                    backgroundColor: '#ff4d4f',
                    color: '#fff',
                    fontSize: 16,
                    padding: 0,
                    cursor: 'pointer',
                    display:
                      hoveredConnectionFile === item.file &&
                      hoveredConnectionCategory === item.category
                        ? 'inline-flex'
                        : 'none',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                  onClick={async (event) => {
                    event.stopPropagation();
                    await handleDeleteConnectionFile(item.category, item.file);
                  }}
                >
                  -
                </button>
              </div>
            ))}
          </div>
        </div>
        <div className="repo-folder">
          <div className="repo-folder-header">{t('configManage.sink')}</div>
          <div className="repo-folder-content">
            {sinkConnectionItems.map((item) => (
              <div
                key={item.key}
                className="repo-file-row"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 8,
                }}
                onMouseEnter={() => {
                  setHoveredConnectionFile(item.file);
                  setHoveredConnectionCategory(item.category);
                }}
                onMouseLeave={() => {
                  setHoveredConnectionFile('');
                  setHoveredConnectionCategory('');
                }}
              >
                <button
                  type="button"
                  className={`repo-file ${
                    activeConnectionCategory === item.category && activeConnectionFile === item.file
                      ? 'is-active'
                      : ''
                  }`}
                  onClick={() =>
                    confirmBeforeSwitch(() => {
                      setActiveConnectionFile(item.file);
                      setActiveConnectionCategory(item.category);
                    })
                  }
                  style={{ flex: 1, textAlign: 'left' }}
                  title={item.file}
                >
                  {item.displayName}
                </button>
                <button
                  type="button"
                  className="repo-file-delete"
                  style={{
                    minWidth: 20,
                    width: 20,
                    height: 20,
                    borderRadius: '50%',
                    border: 'none',
                    backgroundColor: '#ff4d4f',
                    color: '#fff',
                    fontSize: 16,
                    padding: 0,
                    cursor: 'pointer',
                    display:
                      hoveredConnectionFile === item.file &&
                      hoveredConnectionCategory === item.category
                        ? 'inline-flex'
                        : 'none',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                  onClick={async (event) => {
                    event.stopPropagation();
                    await handleDeleteConnectionFile(item.category, item.file);
                  }}
                >
                  -
                </button>
              </div>
            ))}
          </div>
        </div>
      </aside>
      <div className="repo-content">
        <div className="repo-toolbar">
          <div className="repo-path">
            {activeConnectionFile
              ? getConnectionLabel(activeConnectionFile)
              : t('configManage.noFileSelected')}
          </div>
          <div className="editor-actions">
            <button type="button" className="btn tertiary" onClick={handleValidate}>
              {t('configManage.validate')}
            </button>
            <button type="button" className="btn primary" onClick={handleSave}>
              {t('configManage.save')}
            </button>
          </div>
        </div>
        <div className="repo-view">
          <CodeEditor
            className="code-area code-area--large repo-doc is-visible"
            value={content}
            onChange={(value) => setContent(value)}
            language="toml"
            theme="vscodeDark"
          />
        </div>
      </div>
    </div>
  );

  return (
    <>
      <aside className="side-nav" data-group="config-manage">
        <h2>{t('configManage.title')}</h2>
        <button
          type="button"
          className={`side-item ${activeKey === RuleType.PARSE ? 'is-active' : ''}`}
          onClick={() => handleNavigation(RuleType.PARSE)}
        >
          {t('configManage.parseConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === RuleType.SOURCE ? 'is-active' : ''}`}
          onClick={() => handleNavigation(RuleType.SOURCE)}
        >
          {t('configManage.sourceConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === RuleType.SINK ? 'is-active' : ''}`}
          onClick={() => handleNavigation(RuleType.SINK)}
        >
          {t('configManage.sinkConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'connection' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('connection')}
        >
          {t('configManage.connectionConfig')}
        </button>
      </aside>

      <section className="page-panels">
        <article className="panel is-visible">
          <header className="panel-header">
            <h2>{getCurrentConfigLabel()}</h2>
          </header>
          <section className="panel-body config-body">
            {activeKey === RuleType.PARSE
              ? renderSingleConfig('wparse.toml')
              : activeKey === RuleType.SOURCE
                ? renderSingleConfig('wpsrc.toml')
                : activeKey === RuleType.SINK
                  ? renderSinkConfig()
                  : renderConnectionConfig()}
          </section>
        </article>
      </section>

      <Modal
        title={t('configManage.selectConfigType')}
        open={addModalVisible}
        onCancel={() => {
          setAddModalVisible(false);
          setNewFileName('');
          setNewDisplayName('');
        }}
        footer={null}
        width={480}
      >
        <div style={{ marginBottom: 20 }}>
          <p style={{ marginBottom: 16, color: 'var(--muted)' }}>{t('configManage.selectConfigTypeDesc')}</p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <button
              type="button"
              className="modal-option"
              style={{
                padding: '16px',
                border: '2px solid',
                borderColor: addModalType === 'source' ? 'var(--primary)' : 'var(--panel-border)',
                borderRadius: '12px',
                background: addModalType === 'source' ? 'rgba(39, 94, 254, 0.08)' : 'white',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
              }}
              onClick={() => setAddModalType('source')}
            >
              <div style={{ fontWeight: 600, marginBottom: 4 }}>{t('configManage.sourceConnect')}</div>
              <div style={{ fontSize: 13, color: 'var(--muted)' }}>{t('configManage.sourceConfigDesc')}</div>
            </button>
            <button
              type="button"
              className="modal-option"
              style={{
                padding: '16px',
                border: '2px solid',
                borderColor: addModalType === 'sink' ? 'var(--primary)' : 'var(--panel-border)',
                borderRadius: '12px',
                background: addModalType === 'sink' ? 'rgba(39, 94, 254, 0.08)' : 'white',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
              }}
              onClick={() => setAddModalType('sink')}
            >
              <div style={{ fontWeight: 600, marginBottom: 4 }}>{t('configManage.sinkConnect')}</div>
              <div style={{ fontSize: 13, color: 'var(--muted)' }}>{t('configManage.sinkConfigDesc')}</div>
            </button>
          </div>
        </div>
        <div style={{ marginBottom: 20 }}>
          <label style={{ display: 'block', marginBottom: 8, fontWeight: 500 }}>
            {t('configManage.configFileName')}
          </label>
          <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--muted)' }}>
            {t('configManage.fileNameRule')}
          </div>
          <Input
            value={newFileName}
            onChange={(e) => setNewFileName(e.target.value)}
            placeholder={t('configManage.fileNamePlaceholder', {
              type: addModalType === 'source' ? t('configManage.source') : t('configManage.sink'),
            })}
          />
        </div>
        <div style={{ marginBottom: 20 }}>
          <label style={{ display: 'block', marginBottom: 8, fontWeight: 500 }}>
            {t('configManage.displayName')}
          </label>
          <Input
            value={newDisplayName}
            onChange={(e) => setNewDisplayName(e.target.value)}
            placeholder={t('configManage.displayNamePlaceholder')}
            onPressEnter={handleCreateConnectionConfigFile}
          />
        </div>
        <div style={{ display: 'flex', gap: 12, justifyContent: 'flex-end' }}>
          <button
            type="button"
            className="btn ghost"
            onClick={() => {
              setAddModalVisible(false);
              setNewFileName('');
              setNewDisplayName('');
            }}
          >
            {t('common.cancel')}
          </button>
          <button type="button" className="btn primary" onClick={handleCreateConnectionConfigFile}>
            {t('common.confirm')}
          </button>
        </div>
      </Modal>

      <ValidateResultModal
        open={validateModalVisible}
        result={validateResult}
        onClose={() => setValidateModalVisible(false)}
      />
    </>
  );
}

export default ConfigManagePage;
