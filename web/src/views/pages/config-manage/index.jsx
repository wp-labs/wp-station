import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input, Menu, message, Modal } from 'antd';
import { RuleType, fetchRuleConfig, validateRuleConfig, saveRuleConfig, fetchConnectionFiles, createConnectionConfigFile, deleteConnectionConfigFile } from '@/services/config';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';

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

/**
 * 配置管理页面
 * 功能：
 * 1. 显示和编辑解析配置和连接配置
 * 2. 支持配置校验和保存
 * 对应原型：pages/views/config-manage/
 */
function ConfigManagePage() {
  const { t } = useTranslation();
  const [activeKey, setActiveKey] = useState(RuleType.PARSE);
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  
  // 连接配置的文件列表
  const [connectionFiles, setConnectionFiles] = useState({ sources: [], sinks: [] });
  const [activeConnectionFile, setActiveConnectionFile] = useState('');
  const [activeConnectionCategory, setActiveConnectionCategory] = useState('source_connect');
  const [hoveredConnectionFile, setHoveredConnectionFile] = useState('');
  const [hoveredConnectionCategory, setHoveredConnectionCategory] = useState('');
  const [connectionSearch, setConnectionSearch] = useState('');
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [addModalType, setAddModalType] = useState('source'); // 'source' or 'sink'
  const [newFileName, setNewFileName] = useState('');
  const [newDisplayName, setNewDisplayName] = useState('');
  
  // 跟踪内容是否已修改
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [originalContent, setOriginalContent] = useState('');
  const [pendingNavigation, setPendingNavigation] = useState(null);

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

  const sortConnectionItems = React.useCallback((items, category) => {
    const orderMap = new Map(CONNECTION_FILE_ORDER.map((file, index) => [file, index]));

    return (items || [])
      .filter((item) => {
        if (!connectionSearch) return true;
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
  }, [connectionSearch]);

  const sourceConnectionItems = React.useMemo(() => {
    return sortConnectionItems(connectionFiles.sources, 'source_connect');
  }, [connectionFiles.sources, sortConnectionItems]);

  const sinkConnectionItems = React.useMemo(() => {
    return sortConnectionItems(connectionFiles.sinks, 'sink_connect');
  }, [connectionFiles.sinks, sortConnectionItems]);

  /**
   * 处理删除连接配置文件
   */
  const handleDeleteConnectionFile = async (category, file) => {
    const filename = file;
    return new Promise((resolve, reject) => {
      Modal.confirm({
        title: t('configManage.deleteConfirm'),
        content: t('configManage.deleteConfirmMessage', { filename }),
        okText: t('common.delete'),
        okButtonProps: { danger: true },
        cancelText: t('common.cancel'),
        onOk: async () => {
          try {
            await deleteConnectionConfigFile({ category, file: filename });
            const refreshed = await fetchConnectionFiles({
              keyword: connectionSearch || undefined,
            });
            setConnectionFiles(refreshed);

            if (activeConnectionFile === filename) {
              const nextItems = [
                ...(refreshed.sources || []).map((item) => ({
                  file: item.file,
                  category: 'source_connect',
                })),
                ...(refreshed.sinks || []).map((item) => ({
                  file: item.file,
                  category: 'sink_connect',
                })),
              ];
              const nextItem = nextItems[0];
              const nextFile = nextItem?.file || '';
              setActiveConnectionFile(nextFile);
              if (nextItem) {
                setActiveConnectionCategory(nextItem.category);
              } else {
                setContent('');
                setOriginalContent('');
                setHasUnsavedChanges(false);
              }
            }

            message.success(t('configManage.deleteSuccess'));
            resolve(true);
          } catch (error) {
            message.error(t('configManage.deleteFailed', { message: error.message }));
            reject(error);
          }
        },
        onCancel: () => {
          resolve(false);
        },
      });
    });
  };

  /**
   * 加载配置内容
   */
  const loadConfig = async () => {
    setLoading(true);
    try {
      if (activeKey === 'connection' && activeConnectionFile) {
        // 连接配置需要根据来源/输出源决定类别
        const category = activeConnectionCategory === 'sink_connect' ? 'sink_connect' : 'source_connect';
        const type = category === 'source_connect' ? RuleType.SOURCE_CONNECT : RuleType.SINK_CONNECT;
        const response = await fetchRuleConfig({ type, file: activeConnectionFile });
        const newContent = response.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
      } else if (activeKey !== 'connection') {
        // 其他配置类型
        const response = await fetchRuleConfig({ type: activeKey });
        const newContent = response.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
      }
    } finally {
      setLoading(false);
    }
  };

  // 初始化连接配置文件列表
  useEffect(() => {
    const initConnectionFiles = async () => {
      try {
        const files = await fetchConnectionFiles({
          keyword: connectionSearch || undefined,
        });
        setConnectionFiles(files);
        const nextItem = files.sources?.length
          ? { file: files.sources[0].file, category: 'source_connect' }
          : files.sinks?.length
            ? { file: files.sinks[0].file, category: 'sink_connect' }
            : null;

        if (nextItem) {
          setActiveConnectionFile(nextItem.file);
          setActiveConnectionCategory(nextItem.category);
        }
      } catch (error) {
        message.error(t('configManage.loadFailed', { message: error.message }));
      }
    };
    if (activeKey === 'connection') {
      initConnectionFiles();
    }
  }, [activeKey]);
  
  // 当配置类型或连接配置文件变化时重新加载
  useEffect(() => {
    loadConfig();
  }, [activeKey, activeConnectionFile, activeConnectionCategory]);

  /**
   * 处理配置校验
   */
  const handleValidate = async () => {
    try {
      let type;
      let file;
      const currentContent = content || '';

      if (activeKey === 'connection') {
        if (!activeConnectionFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }
        const category =
          activeConnectionCategory === 'sink_connect'
            ? RuleType.SINK_CONNECT
            : RuleType.SOURCE_CONNECT;
        type = category;
        file = activeConnectionFile;
      } else {
        // 解析配置使用 parse 类型，文件名固定为 wparse.toml
        type = RuleType.PARSE;
        file = 'wparse.toml';
      }

      // 调用服务层校验配置（使用对象参数）
      const response = await validateRuleConfig({ type, file, content: currentContent });

      if (response.valid) {
        const warnings = response.warnings || 0;
        const statusColor = warnings === 0 ? '#52c41a' : '#faad14';
        const statusIcon = warnings === 0 ? '✓' : '⚠';
        const statusText = warnings === 0 ? t('ruleManage.validateSuccess') : t('ruleManage.validateWarning');
        const now = new Date().toLocaleString('zh-CN');
        const lineCount = response.lines ?? (currentContent ? currentContent.split('\n').length : 0);
        const typeLabel = activeKey === 'connection' 
          ? (activeConnectionCategory === 'sink_connect' ? t('configManage.sinkConnect') : t('configManage.sourceConnect'))
          : t('configManage.parseConfig');
        
        Modal.info({
          icon: null,
          okText: t('common.confirm'),
          width: 580,
          title: t('ruleManage.validationResult'),
          content: (
            <div>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 12,
                  marginBottom: 20,
                  padding: 16,
                  background: warnings === 0 ? '#f6ffed' : '#fffbe6',
                  borderLeft: `3px solid ${statusColor}`,
                  borderRadius: 8,
                }}
              >
                <span style={{ fontSize: 28, color: statusColor }}>{statusIcon}</span>
                <div>
                  <div style={{ fontSize: 16, fontWeight: 600, color: statusColor, marginBottom: 4 }}>{statusText}</div>
                  <div style={{ fontSize: 13, color: '#666' }}>{t('ruleManage.conforms', { type: typeLabel })}</div>
                </div>
              </div>
              <div style={{ background: '#fafafa', borderRadius: 8, padding: 16 }}>
                <table style={{ width: '100%', fontSize: 13, lineHeight: 2 }}>
                  <tbody>
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.fileName')}</td>
                      <td style={{ fontWeight: 500 }}>{file}</td>
                    </tr>
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.codeLines')}</td>
                      <td style={{ fontWeight: 500 }}>{t('ruleManage.lines', { count: lineCount })}</td>
                    </tr>
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.syntaxCheck')}</td>
                      <td style={{ color: '#52c41a', fontWeight: 500 }}>{t('ruleManage.passed')}</td>
                    </tr>
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.formatCheck')}</td>
                      <td style={{ color: '#52c41a', fontWeight: 500 }}>{t('ruleManage.passed')}</td>
                    </tr>
                    {warnings > 0 ? (
                      <tr>
                        <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.warningInfo')}</td>
                        <td style={{ color: '#faad14', fontWeight: 500 }}>{t('ruleManage.warnings', { count: warnings })}</td>
                      </tr>
                    ) : null}
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.validationTime')}</td>
                      <td style={{ fontWeight: 500 }}>{now}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          ),
        });
      } else {
        Modal.error({
          title: t('ruleManage.validateFailed'),
          content: response.message || t('ruleManage.validateFailedMessage'),
        });
      }
    } catch (error) {
      message.error(t('ruleManage.validateFailed') + '：' + (error.message || '未知错误'));
    }
  };

  /**
   * 处理配置保存
   */
  const handleSave = async () => {
    const currentMenuItem = menuItems.find((item) => item.key === activeKey);
    const configLabel = currentMenuItem?.label || activeKey;
    const fileName = activeKey === 'connection' ? activeConnectionFile : 'wparse.toml';
    
    try {
      if (activeKey === 'connection') {
        if (!activeConnectionFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }
        const category = activeConnectionCategory === 'sink_connect' ? 'sink_connect' : 'source_connect';
        const type = category;
        await saveRuleConfig({
          type,
          file: activeConnectionFile,
          content,
        });
      } else {
        await saveRuleConfig({ 
          type: RuleType.PARSE, 
          file: fileName,
          content, 
        });
      }

      // 保存成功后重置未保存状态
      setOriginalContent(content);
      setHasUnsavedChanges(false);

      // 使用与规则配置页一致的保存成功弹窗样式
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
      message.error(t('configManage.saveFailed') + '：' + error.message);
    }
  };

  const menuItems = [
    { key: RuleType.PARSE, label: t('configManage.parseConfig') },
    { key: 'connection', label: t('configManage.connectionConfig') },
  ];

  // 获取配置标题（与旧版本一致）
  const getConfigTitle = () => {
    if (activeKey === 'parse') return t('configManage.parseConfig');
    if (activeKey === 'connection') return t('configManage.connectionConfig');
    return t('configManage.title');
  };

  // 获取配置文件名
  const getConfigFileName = () => {
    if (activeKey === 'connection') {
      return activeConnectionFile || '';
    }
    return '';
  };

  /**
   * 处理页面切换
   */
  const handleNavigation = (newKey) => {
    if (hasUnsavedChanges) {
      setPendingNavigation(newKey);
      Modal.confirm({
        title: t('configManage.leaveConfirm'),
        content: t('configManage.leaveConfirmMessage'),
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: () => {
          setHasUnsavedChanges(false);
          setActiveKey(newKey);
          setPendingNavigation(null);
        },
        onCancel: () => {
          setPendingNavigation(null);
        },
      });
    } else {
      setActiveKey(newKey);
    }
  };

  /**
   * 监听内容变化
   */
  useEffect(() => {
    if (content !== originalContent) {
      setHasUnsavedChanges(true);
    } else {
      setHasUnsavedChanges(false);
    }
  }, [content, originalContent]);

  return (
    <>
      {/* 左侧侧边栏 */}
      <aside className="side-nav" data-group="config-manage">
        <h2>{t('configManage.title')}</h2>
        <button
          type="button"
          className={`side-item ${activeKey === 'parse' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('parse')}
        >
          {t('configManage.parseConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'connection' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('connection')}
        >
          {t('configManage.connectionConfig')}
        </button>
      </aside>

      {/* 右侧配置内容区 */}
      <section className="page-panels">
        <article className="panel is-visible">
          <header className="panel-header">
            <h2>{getConfigTitle()}</h2>
          </header>
          <section className="panel-body config-body">
            {/* 解析配置 - 单一配置布局 */}
            {activeKey === 'parse' ? (
              <div className="single-config">
                <header className="single-config-header">
                  <span className="single-config-name">wparse.toml</span>
                  <div className="single-config-actions">
                    <button type="button" className="btn tertiary" onClick={handleValidate}>
                      {t('configManage.validate')}
                    </button>
                    <button type="button" className="btn primary" onClick={() => {
                      handleSave();
                      setHasUnsavedChanges(false);
                      setOriginalContent(content);
                    }}>
                      {t('configManage.save')}
                    </button>
                  </div>
                </header>
                <CodeEditor
                  className="code-area code-area--full"
                  value={content}
                  onChange={(value) => setContent(value)}
                  language="toml"
                  theme="vscodeDark"
                />
              </div>
            ) : activeKey === 'connection' ? (
              /* 连接配置 - 左右布局（类似 wpl 配置） */
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
                        fetchConnectionFiles({ keyword: value || undefined })
                          .then((files) => {
                            setConnectionFiles(files);

                            // 如果当前选中的文件不在新的结果集中，则自动切换到第一个匹配项
                            const isSink = activeConnectionCategory === 'sink_connect';
                            const list = isSink ? files.sinks : files.sources;
                            if (!list.some((item) => item.file === activeConnectionFile)) {
                              const nextItem = list[0] || null;
                              const nextFile = nextItem?.file || '';
                              setActiveConnectionFile(nextFile);
                              if (nextItem) {
                                setActiveConnectionCategory(isSink ? 'sink_connect' : 'source_connect');
                              } else {
                                setContent('');
                                setOriginalContent('');
                                setHasUnsavedChanges(false);
                              }
                            }
                          })
                          .catch((error) => {
                            message.error(t('configManage.loadFailed', { message: error.message }));
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
                            onClick={() => {
                              const isSameActive =
                                activeConnectionCategory === item.category &&
                                activeConnectionFile === item.file;

                              if (hasUnsavedChanges && !isSameActive) {
                                Modal.confirm({
                                  title: t('configManage.leaveConfirm'),
                                  content: t('configManage.leaveConfirmMessage'),
                                  okText: t('common.confirm'),
                                  cancelText: t('common.cancel'),
                                  onOk: () => {
                                    setActiveConnectionFile(item.file);
                                    setActiveConnectionCategory(item.category);
                                  },
                                });
                              } else {
                                setActiveConnectionFile(item.file);
                                setActiveConnectionCategory(item.category);
                              }
                            }}
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
                            onClick={() => {
                              const isSameActive =
                                activeConnectionCategory === item.category &&
                                activeConnectionFile === item.file;

                              if (hasUnsavedChanges && !isSameActive) {
                                Modal.confirm({
                                  title: t('configManage.leaveConfirm'),
                                  content: t('configManage.leaveConfirmMessage'),
                                  okText: t('common.confirm'),
                                  cancelText: t('common.cancel'),
                                  onOk: () => {
                                    setActiveConnectionFile(item.file);
                                    setActiveConnectionCategory(item.category);
                                  },
                                });
                              } else {
                                setActiveConnectionFile(item.file);
                                setActiveConnectionCategory(item.category);
                              }
                            }}
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
                        ? activeConnectionFile
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
            ) : null}
          </section>
        </article>
      </section>

      {/* 新增连接配置模态框 */}
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
              <div style={{ fontWeight: 600, marginBottom: 4 }}>{t('configManage.sourceConfig')}</div>
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
              <div style={{ fontWeight: 600, marginBottom: 4 }}>{t('configManage.sinkConfig')}</div>
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
            placeholder={t('configManage.fileNamePlaceholder', { type: addModalType === 'source' ? t('configManage.source') : t('configManage.sink') })}
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
            onPressEnter={() => {
              if (newFileName.trim() && newDisplayName.trim()) {
                const normalized = newFileName.trim();
                const normalizedDisplayName = newDisplayName.trim();
                const category = addModalType === 'source' ? 'source_connect' : 'sink_connect';
                createConnectionConfigFile({
                  category,
                  file: normalized,
                  displayName: normalizedDisplayName,
                })
                  .then(async () => {
                    const refreshed = await fetchConnectionFiles({
                      keyword: connectionSearch || undefined,
                    });
                    setConnectionFiles(refreshed);
                    setActiveConnectionFile(normalized);
                    setActiveConnectionCategory(category);
                    message.success(t('configManage.createSuccess', { filename: normalizedDisplayName }));
                    setAddModalVisible(false);
                    setNewFileName('');
                    setNewDisplayName('');
                  })
                  .catch((error) => {
                    message.error(t('configManage.createFailed', { message: error.message }));
                  });
              }
            }}
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
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              if (newFileName.trim() && newDisplayName.trim()) {
                const normalized = newFileName.trim();
                const normalizedDisplayName = newDisplayName.trim();
                const category = addModalType === 'source' ? 'source_connect' : 'sink_connect';
                createConnectionConfigFile({
                  category,
                  file: normalized,
                  displayName: normalizedDisplayName,
                })
                  .then(async () => {
                    const refreshed = await fetchConnectionFiles({
                      keyword: connectionSearch || undefined,
                    });
                    setConnectionFiles(refreshed);
                    setActiveConnectionFile(normalized);
                    setActiveConnectionCategory(category);
                    message.success(t('configManage.createSuccess', { filename: normalizedDisplayName }));
                    setAddModalVisible(false);
                    setNewFileName('');
                    setNewDisplayName('');
                  })
                  .catch((error) => {
                    message.error(t('configManage.createFailed', { message: error.message }));
                  });
              } else if (!newFileName.trim()) {
                message.warning(t('configManage.enterFileName'));
              } else {
                message.warning(t('configManage.enterDisplayName'));
              }
            }}
          >
            {t('common.confirm')}
          </button>
        </div>
      </Modal>
    </>
  );
}

export default ConfigManagePage;
