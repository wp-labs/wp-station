import React, { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input, Pagination, message, Modal } from 'antd';
import {
  RuleType,
  fetchConfigTemplates,
  fetchRuleConfig,
  validateRuleConfig,
  saveRuleConfig,
  fetchRuleFiles,
  fetchConnectionFiles,
  createConfigFile,
  deleteConfigFile,
  createConnectionConfigFile,
  deleteConnectionConfigFile,
  renderConfigTemplate,
} from '@/services/config';
import { tomlCodeFormat } from '@/services/debug';
import { useSystem } from '@/contexts/SystemContext';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';
import ValidateResultModal from '@/components/ValidateResultModal';
const TEMPLATE_PAGE_SIZE = 10;

const sortSinkItems = (items = []) => {
  return items
    .map((item) => {
      if (!item?.file) {
        return null;
      }

      return {
        file: item.file,
        sortOrder: typeof item.sortOrder === 'number' ? item.sortOrder : Number.MAX_SAFE_INTEGER,
        displayName:
          typeof item.displayName === 'string' && item.displayName.trim()
            ? item.displayName.trim()
            : item.file,
      };
    })
    .filter(Boolean)
    .sort((a, b) => {
      if (a.sortOrder !== b.sortOrder) {
        return a.sortOrder - b.sortOrder;
      }

      return a.displayName.localeCompare(b.displayName, 'zh-CN');
    });
};

const splitSinkFile = (file = '') => {
  const normalized = String(file || '').trim();
  if (!normalized) {
    return { group: 'root', name: '' };
  }
  const parts = normalized.split('/').filter(Boolean);
  if (parts.length <= 1) {
    return { group: 'root', name: parts[0] || normalized };
  }
  return {
    group: parts[0],
    name: parts.slice(1).join('/'),
  };
};

const displayConfigFileName = (file = '') => {
  const normalized = String(file || '').trim();
  if (!normalized) {
    return '';
  }
  const parts = normalized.split('/').filter(Boolean);
  return parts[parts.length - 1] || normalized;
};

function ConfigManagePage() {
  const { t } = useTranslation();
  const { currentSystem, registerBeforeSystemSwitch } = useSystem();
  const [activeKey, setActiveKey] = useState(RuleType.PARSE);
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [sinkFiles, setSinkFiles] = useState([]);
  const [sourceFiles, setSourceFiles] = useState([]);
  const [activeSourceFile, setActiveSourceFile] = useState('');
  const [hoveredSourceFile, setHoveredSourceFile] = useState('');
  const [activeSinkFile, setActiveSinkFile] = useState('');
  const [sinkExpandedGroups, setSinkExpandedGroups] = useState([]);
  const [hoveredSinkFile, setHoveredSinkFile] = useState('');
  const [sinkAddModalVisible, setSinkAddModalVisible] = useState(false);
  const [newSinkFileName, setNewSinkFileName] = useState('');
  const [sourceAddModalVisible, setSourceAddModalVisible] = useState(false);
  const [newSourceFileName, setNewSourceFileName] = useState('');
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
  const [templateModalVisible, setTemplateModalVisible] = useState(false);
  const [templateScope, setTemplateScope] = useState(RuleType.SOURCE);
  const [templateList, setTemplateList] = useState([]);
  const [selectedTemplateId, setSelectedTemplateId] = useState('');
  const [templateLoading, setTemplateLoading] = useState(false);
  const [templatePreview, setTemplatePreview] = useState(null);
  const [templatePreviewLoading, setTemplatePreviewLoading] = useState(false);
  const [templatePage, setTemplatePage] = useState(1);
  const [templateSearch, setTemplateSearch] = useState('');
  const [currentSingleConfigFile, setCurrentSingleConfigFile] = useState('');

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
          sortOrder:
            typeof item.sortOrder === 'number' ? item.sortOrder : Number.MAX_SAFE_INTEGER,
          displayName: item.displayName || item.file,
        }))
        .sort((a, b) => {
          if (a.sortOrder !== b.sortOrder) {
            return a.sortOrder - b.sortOrder;
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
  const displayedSourceFiles = useMemo(
    () =>
      sortSinkItems(
        sourceFiles.map((item) => ({
          ...item,
          displayName: item.displayName || displayConfigFileName(item.file),
        })),
      ),
    [sourceFiles],
  );
  const sinkGroups = useMemo(() => {
    const groups = new Map();
    displayedSinkFiles.forEach((item) => {
      const { group, name } = splitSinkFile(item.file);
      const nextItem = {
        ...item,
        group,
        name: name || item.displayName || item.file,
      };
      const current = groups.get(group) || [];
      current.push(nextItem);
      groups.set(group, current);
    });

    const orderedGroups = ['business.d', 'infra.d', 'root'];
    return Array.from(groups.entries())
      .sort((a, b) => {
        const aOrder = orderedGroups.indexOf(a[0]);
        const bOrder = orderedGroups.indexOf(b[0]);
        const normalizedA = aOrder >= 0 ? aOrder : Number.MAX_SAFE_INTEGER;
        const normalizedB = bOrder >= 0 ? bOrder : Number.MAX_SAFE_INTEGER;
        if (normalizedA !== normalizedB) {
          return normalizedA - normalizedB;
        }
        return a[0].localeCompare(b[0], 'zh-CN');
      })
      .map(([group, items]) => ({ group, items }));
  }, [displayedSinkFiles]);
  const filteredTemplateList = useMemo(() => {
    const keyword = templateSearch.trim().toLowerCase();
    if (!keyword) {
      return templateList;
    }

    return templateList.filter((item) =>
      [item?.displayName, item?.templateFile, item?.connect]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(keyword)),
    );
  }, [templateList, templateSearch]);
  const selectedTemplate = useMemo(
    () => filteredTemplateList.find((item) => item?.templateId === selectedTemplateId) || null,
    [filteredTemplateList, selectedTemplateId],
  );
  const pagedTemplateList = useMemo(() => {
    const start = (templatePage - 1) * TEMPLATE_PAGE_SIZE;
    return filteredTemplateList.slice(start, start + TEMPLATE_PAGE_SIZE);
  }, [filteredTemplateList, templatePage]);
  const supportsSinkTemplate = activeSinkFile.startsWith('business.d/');

  useEffect(() => {
    setSinkExpandedGroups((prev) => {
      const groupKeys = sinkGroups.map((item) => item.group);
      if (!prev.length) {
        return groupKeys;
      }
      const next = prev.filter((group) => groupKeys.includes(group));
      const missing = groupKeys.filter((group) => !next.includes(group));
      return [...next, ...missing];
    });
  }, [sinkGroups]);

  const getDefaultFileForKey = (key) => {
    if (key === RuleType.PARSE || key === RuleType.SOURCE) {
      return key === RuleType.SOURCE ? activeSourceFile || '' : currentSingleConfigFile || '';
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

  const loadSourceFiles = async () => {
    const response = await fetchRuleFiles({ type: RuleType.SOURCE });
    const files = Array.isArray(response?.items) ? response.items : [];
    setSourceFiles(files);
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

      if (key === RuleType.SOURCE) {
        const normalized = sortSinkItems(
          (files || []).map((item) => ({
            ...item,
            displayName: item.displayName || displayConfigFileName(item.file),
          })),
        );
        const activeExists = normalized.some((item) => item.file === activeSourceFile);
        if (!activeExists) {
          setActiveSourceFile(normalized[0]?.file || '');
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
    [activeConnectionCategory, activeConnectionFile, activeSinkFile, activeSourceFile],
  );

  const loadConfig = async () => {
    setLoading(true);
    try {
      if (activeKey === RuleType.SOURCE) {
        setCurrentSingleConfigFile('');
        if (!activeSourceFile) {
          setContent('');
          setOriginalContent('');
          setHasUnsavedChanges(false);
          return;
        }
        const response = await fetchRuleConfig({
          type: RuleType.SOURCE,
          file: activeSourceFile,
        });
        const newContent = response?.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
        return;
      }

      if (activeKey === RuleType.SINK) {
        setCurrentSingleConfigFile('');
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
        setCurrentSingleConfigFile('');
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
      setCurrentSingleConfigFile(response?.file || '');
      const newContent = response?.content || '';
      setContent(newContent);
      setOriginalContent(newContent);
      setHasUnsavedChanges(false);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (activeKey === RuleType.SOURCE) {
      loadSourceFiles()
        .then((files) => ensureSelectionAfterListLoad(activeKey, files))
        .catch((error) => {
          message.error(t('configManage.loadFailed', { message: error.message }));
        });
      return;
    }

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
  }, [activeKey, activeSourceFile, activeSinkFile, activeConnectionFile, activeConnectionCategory]);

  useEffect(() => {
    setHasUnsavedChanges(content !== originalContent);
  }, [content, originalContent]);

  useEffect(() => {
    if (!templateModalVisible || !selectedTemplateId) {
      setTemplatePreview(null);
      return undefined;
    }

    let canceled = false;
    setTemplatePreviewLoading(true);

    renderConfigTemplate({
      scope: templateScope,
      templateId: selectedTemplateId,
      content: content || '',
    })
      .then((response) => {
        if (!canceled) {
          setTemplatePreview(response);
        }
      })
      .catch((error) => {
        if (!canceled) {
          setTemplatePreview(null);
          message.error(t('configManage.templateRenderFailed', { message: error.message }));
        }
      })
      .finally(() => {
        if (!canceled) {
          setTemplatePreviewLoading(false);
        }
      });

    return () => {
      canceled = true;
    };
  }, [content, selectedTemplateId, templateModalVisible, templateScope, t]);

  useEffect(() => {
    if (!templateModalVisible) {
      return;
    }

    const totalPages = Math.max(1, Math.ceil(filteredTemplateList.length / TEMPLATE_PAGE_SIZE));
    if (templatePage > totalPages) {
      setTemplatePage(totalPages);
    }
  }, [filteredTemplateList.length, templateModalVisible, templatePage]);

  useEffect(() => {
    if (!templateModalVisible) {
      return;
    }

    if (!filteredTemplateList.length) {
      if (selectedTemplateId) {
        setSelectedTemplateId('');
      }
      return;
    }

    const selectedInCurrentPage = pagedTemplateList.some(
      (item) => item.templateId === selectedTemplateId,
    );
    if (!selectedInCurrentPage) {
      setSelectedTemplateId(pagedTemplateList[0].templateId);
    }
  }, [filteredTemplateList.length, pagedTemplateList, selectedTemplateId, templateModalVisible]);

  const confirmBeforeLeave = React.useCallback(() => {
    if (!hasUnsavedChanges) {
      return Promise.resolve(true);
    }

    return new Promise((resolve) => {
      Modal.confirm({
        title: t('configManage.leaveConfirm'),
        content: t('configManage.leaveConfirmMessage'),
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: () => {
          setHasUnsavedChanges(false);
          resolve(true);
        },
        onCancel: () => resolve(false),
      });
    });
  }, [hasUnsavedChanges, t]);

  useEffect(() => registerBeforeSystemSwitch(() => confirmBeforeLeave()), [
    confirmBeforeLeave,
    registerBeforeSystemSwitch,
  ]);

  const confirmBeforeSwitch = (onConfirm) => {
    confirmBeforeLeave().then((confirmed) => {
      if (confirmed) {
        onConfirm();
      }
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

      if (activeKey === RuleType.SOURCE && !activeSourceFile) {
        message.warning(t('configManage.noFileSelected'));
        return;
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
      if (activeKey === RuleType.SOURCE) {
        if (!activeSourceFile) {
          message.warning(t('configManage.noFileSelected'));
          return;
        }
        await saveRuleConfig({
          type: RuleType.SOURCE,
          file: activeSourceFile,
          content,
        });
      } else if (activeKey === RuleType.SINK) {
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

  const handleFormat = async () => {
    if (!content || content.trim() === '') {
      message.warning(t('common.noFormatContent'));
      return;
    }

    try {
      const result = await tomlCodeFormat(content);
      const formattedCode = result?.toml_code || '';

      if (formattedCode && formattedCode !== content) {
        setContent(formattedCode);
        message.success(t('ruleManage.format'));
        return;
      }

      message.info(t('ruleManage.format'));
    } catch (error) {
      message.error(error?.message || t('debug.toml.formatError'));
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

  const handleCreateSinkConfigFile = async () => {
    const normalized = newSinkFileName.trim();

    if (!normalized) {
      message.warning(t('configManage.enterSinkFileName'));
      return;
    }

    const fileName = normalized.endsWith('.toml') ? normalized : `${normalized}.toml`;
    const targetFile = `business.d/${fileName}`;

    try {
      await createConfigFile({
        type: RuleType.SINK,
        file: targetFile,
      });
      await loadSinkFiles();
      setActiveSinkFile(targetFile);
      setSinkExpandedGroups((prev) =>
        prev.includes('business.d') ? prev : [...prev, 'business.d'],
      );
      setSinkAddModalVisible(false);
      setNewSinkFileName('');
      message.success(t('configManage.createSuccess', { filename: fileName }));
    } catch (error) {
      message.error(t('configManage.createFailed', { message: error.message }));
    }
  };

  const handleCreateSourceConfigFile = async () => {
    const normalized = newSourceFileName.trim();

    if (!normalized) {
      message.warning(t('configManage.enterFileName'));
      return;
    }

    const fileName = normalized.endsWith('.toml') ? normalized : `${normalized}.toml`;

    try {
      await createConfigFile({
        type: RuleType.SOURCE,
        file: fileName,
      });
      const refreshed = await loadSourceFiles();
      setActiveSourceFile(fileName);
      ensureSelectionAfterListLoad(RuleType.SOURCE, refreshed);
      setSourceAddModalVisible(false);
      setNewSourceFileName('');
      message.success(t('configManage.createSuccess', { filename: fileName }));
    } catch (error) {
      message.error(t('configManage.createFailed', { message: error.message }));
    }
  };

  const handleDeleteSourceFile = async (file) =>
    new Promise((resolve, reject) => {
      Modal.confirm({
        title: t('configManage.deleteConfirm'),
        content: t('configManage.deleteConfirmMessage', { filename: file }),
        okText: t('common.delete'),
        okButtonProps: { danger: true },
        cancelText: t('common.cancel'),
        onOk: async () => {
          try {
            await deleteConfigFile({
              type: RuleType.SOURCE,
              file,
            });
            const refreshed = await loadSourceFiles();
            ensureSelectionAfterListLoad(RuleType.SOURCE, refreshed);
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

  const handleDeleteSinkFile = async (file) =>
    new Promise((resolve, reject) => {
      Modal.confirm({
        title: t('configManage.deleteConfirm'),
        content: t('configManage.deleteConfirmMessage', { filename: file }),
        okText: t('common.delete'),
        okButtonProps: { danger: true },
        cancelText: t('common.cancel'),
        onOk: async () => {
          try {
            await deleteConfigFile({
              type: RuleType.SINK,
              file,
            });
            const refreshed = await loadSinkFiles();
            ensureSelectionAfterListLoad(RuleType.SINK, refreshed);
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

  const toggleSinkGroup = (group) => {
    setSinkExpandedGroups((prev) =>
      prev.includes(group) ? prev.filter((item) => item !== group) : [...prev, group],
    );
  };

  const getSinkGroupLabel = (group) => {
    if (group === 'business.d') {
      return t('configManage.sinkBusinessGroup');
    }
    if (group === 'infra.d') {
      return t('configManage.sinkInfraGroup');
    }
    return t('configManage.sinkOtherGroup');
  };

  const closeTemplateModal = () => {
    setTemplateModalVisible(false);
    setTemplateList([]);
    setSelectedTemplateId('');
    setTemplatePreview(null);
    setTemplateLoading(false);
    setTemplatePreviewLoading(false);
    setTemplatePage(1);
    setTemplateSearch('');
  };

  const openTemplateModal = async (scope) => {
    if (scope === RuleType.SINK && !supportsSinkTemplate) {
      message.warning(t('configManage.templateBusinessOnly'));
      return;
    }

    setTemplateScope(scope);
    setTemplateModalVisible(true);
    setTemplateList([]);
    setSelectedTemplateId('');
    setTemplatePreview(null);
    setTemplateLoading(true);
    setTemplatePage(1);
    setTemplateSearch('');

    try {
      const response = await fetchConfigTemplates(scope);
      const items = Array.isArray(response?.items) ? response.items : [];
      setTemplateList(items);
      setSelectedTemplateId(items[0]?.templateId || '');
    } catch (error) {
      message.error(t('configManage.templateLoadFailed', { message: error.message }));
      closeTemplateModal();
    } finally {
      setTemplateLoading(false);
    }
  };

  const handleApplyTemplate = () => {
    if (!templatePreview?.content) {
      message.warning(t('configManage.templateNoData'));
      return;
    }

    setContent(templatePreview.content);
    closeTemplateModal();
    message.success(
      t('configManage.templateApplySuccess', {
        name: templatePreview.displayName || templatePreview.templateId,
      }),
    );
  };

  const handleTemplatePageChange = (page) => {
    setTemplatePage(page);
  };

  const renderFieldTags = (items = [], tone = 'default') =>
    items.length ? (
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        {items.map((item) => (
          <span
            key={item}
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              padding: '4px 10px',
              borderRadius: 999,
              fontSize: 12,
              lineHeight: 1,
              border: '1px solid',
              borderColor:
                tone === 'warning'
                  ? 'rgba(250, 173, 20, 0.35)'
                  : tone === 'muted'
                    ? 'rgba(0, 0, 0, 0.08)'
                    : 'rgba(39, 94, 254, 0.18)',
              background:
                tone === 'warning'
                  ? 'rgba(250, 173, 20, 0.08)'
                  : tone === 'muted'
                    ? 'rgba(0, 0, 0, 0.03)'
                    : 'rgba(39, 94, 254, 0.08)',
              color: tone === 'warning' ? '#ad6800' : 'var(--text-primary)',
            }}
          >
            {item}
          </span>
        ))}
      </div>
    ) : (
      <div style={{ fontSize: 12, color: 'var(--muted)' }}>{t('common.noData')}</div>
    );

  const renderSingleConfig = (fileName, language = 'toml', extraActions = null) => (
    <div className="single-config">
      <header className="single-config-header">
        <span className="single-config-name">{fileName}</span>
        <div className="single-config-actions">
          {extraActions}
          {language === 'toml' ? (
            <button type="button" className="btn ghost" onClick={handleFormat}>
              {t('ruleManage.format')}
            </button>
          ) : null}
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
        <div className="repo-tree-header">
          <h3>{t('configManage.configFiles')}</h3>
          <button
            type="button"
            className="btn ghost repo-add-btn"
            onClick={() => setSinkAddModalVisible(true)}
          >
            {t('configManage.add')}
          </button>
        </div>
        <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
          {sinkGroups.map((group) => {
            const expanded = sinkExpandedGroups.includes(group.group);
            return (
              <div key={group.group} className="repo-file-group">
                <button
                  type="button"
                  className="repo-file repo-file--folder"
                  onClick={() => toggleSinkGroup(group.group)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                  }}
                >
                  <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span aria-hidden="true">{expanded ? '📂' : '📁'}</span>
                    {getSinkGroupLabel(group.group)}
                  </span>
                  <span style={{ fontSize: 12, color: '#999' }}>{group.items.length}</span>
                </button>
                {expanded ? (
                  <div style={{ marginLeft: 16, marginTop: 4 }}>
                    {group.items.map((item) => {
                      const canDelete = item.group === 'business.d';
                      return (
                        <div
                          key={item.file}
                          className="repo-file-row"
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                            gap: 8,
                            position: 'relative',
                          }}
                          onMouseEnter={() => setHoveredSinkFile(item.file)}
                          onMouseLeave={() => setHoveredSinkFile('')}
                        >
                          <button
                            type="button"
                            className={`repo-file ${activeSinkFile === item.file ? 'is-active' : ''}`}
                            onClick={() =>
                              confirmBeforeSwitch(() => {
                                setActiveSinkFile(item.file);
                              })
                            }
                            style={{
                              flex: 1,
                              textAlign: 'left',
                              paddingLeft: 18,
                              paddingRight:
                                canDelete && hoveredSinkFile === item.file ? '28px' : '12px',
                            }}
                          >
                            {item.name}
                          </button>
                          {canDelete ? (
                            <button
                              type="button"
                              className="repo-file-delete"
                              style={{
                                position: 'absolute',
                                right: '4px',
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
                                display: hoveredSinkFile === item.file ? 'inline-flex' : 'none',
                                alignItems: 'center',
                                justifyContent: 'center',
                              }}
                              onClick={async (event) => {
                                event.stopPropagation();
                                await handleDeleteSinkFile(item.file);
                              }}
                            >
                              -
                            </button>
                          ) : null}
                        </div>
                      );
                    })}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </aside>

      <div className="repo-content">
        <div className="repo-toolbar">
          <div className="repo-path">{activeSinkFile || t('configManage.noFileSelected')}</div>
          <div className="editor-actions">
            <button
              type="button"
              className="btn ghost"
              onClick={() => openTemplateModal(RuleType.SINK)}
              disabled={!supportsSinkTemplate}
              title={!supportsSinkTemplate ? t('configManage.templateBusinessOnly') : undefined}
              style={
                !supportsSinkTemplate
                  ? { opacity: 0.55, cursor: 'not-allowed' }
                  : undefined
              }
            >
              {t('configManage.addSinkTemplate')}
            </button>
            <button type="button" className="btn ghost" onClick={handleFormat}>
              {t('ruleManage.format')}
            </button>
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

  const renderSourceConfig = () => (
    <div className="repo-layout" data-repo="source">
      <aside className="repo-tree" aria-label="来源配置文件列表">
        <div className="repo-tree-header">
          <h3>{t('configManage.configFiles')}</h3>
          <button
            type="button"
            className="btn ghost repo-add-btn"
            onClick={() => setSourceAddModalVisible(true)}
          >
            {t('configManage.add')}
          </button>
        </div>
        <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
          {displayedSourceFiles.map((item) => (
            <div
              key={item.file}
              className="repo-file-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 8,
                position: 'relative',
              }}
              onMouseEnter={() => setHoveredSourceFile(item.file)}
              onMouseLeave={() => setHoveredSourceFile('')}
            >
              <button
                type="button"
                className={`repo-file ${activeSourceFile === item.file ? 'is-active' : ''}`}
                onClick={() =>
                  confirmBeforeSwitch(() => {
                    setActiveSourceFile(item.file);
                  })
                }
                style={{
                  flex: 1,
                  textAlign: 'left',
                  paddingRight: hoveredSourceFile === item.file ? '28px' : '12px',
                }}
                title={item.file}
              >
                {item.displayName || displayConfigFileName(item.file)}
              </button>
              <button
                type="button"
                className="repo-file-delete"
                style={{
                  position: 'absolute',
                  right: '4px',
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
                  display: hoveredSourceFile === item.file ? 'inline-flex' : 'none',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
                onClick={async (event) => {
                  event.stopPropagation();
                  await handleDeleteSourceFile(item.file);
                }}
              >
                -
              </button>
            </div>
          ))}
        </div>
      </aside>

      <div className="repo-content">
        <div className="repo-toolbar">
          <div className="repo-path">
            {activeSourceFile
              ? displayConfigFileName(activeSourceFile)
              : t('configManage.noFileSelected')}
          </div>
          <div className="editor-actions">
            <button
              type="button"
              className="btn ghost"
              onClick={() => openTemplateModal(RuleType.SOURCE)}
              disabled={!activeSourceFile}
            >
              {t('configManage.addSourceTemplate')}
            </button>
            <button type="button" className="btn ghost" onClick={handleFormat}>
              {t('ruleManage.format')}
            </button>
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
          <div className="repo-folder-header">
            <span className="repo-folder-title">
              <span aria-hidden="true">📁</span>
              {t('configManage.source')}
            </span>
            <span className="repo-folder-count">{sourceConnectionItems.length}</span>
          </div>
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
          <div className="repo-folder-header">
            <span className="repo-folder-title">
              <span aria-hidden="true">📁</span>
              {t('configManage.sink')}
            </span>
            <span className="repo-folder-count">{sinkConnectionItems.length}</span>
          </div>
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
            <button type="button" className="btn ghost" onClick={handleFormat}>
              {t('ruleManage.format')}
            </button>
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
              ? renderSingleConfig(currentSingleConfigFile || t('configManage.parseConfig'))
              : activeKey === RuleType.SOURCE
                ? renderSourceConfig()
                : activeKey === RuleType.SINK
                  ? renderSinkConfig()
                  : renderConnectionConfig()}
          </section>
        </article>
      </section>

      <Modal
        title={
          templateScope === RuleType.SINK
            ? t('configManage.selectSinkTemplate')
            : t('configManage.selectSourceTemplate')
        }
        open={templateModalVisible}
        onCancel={closeTemplateModal}
        footer={null}
        width={960}
      >
        <div style={{ display: 'flex', gap: 16, minHeight: 420 }}>
          <aside
            style={{
              width: 260,
              borderRight: '1px solid var(--panel-border)',
              paddingRight: 16,
            }}
          >
            <div style={{ marginBottom: 12, fontSize: 13, color: 'var(--muted)' }}>
              {templateScope === RuleType.SINK
                ? t('configManage.selectSinkTemplateDesc')
                : t('configManage.selectSourceTemplateDesc')}
            </div>
            <div style={{ marginBottom: 12 }}>
              <Input
                size="small"
                allowClear
                placeholder={t('configManage.templateSearchPlaceholder')}
                value={templateSearch}
                onChange={(event) => {
                  setTemplateSearch(event.target.value);
                  setTemplatePage(1);
                }}
              />
            </div>
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
                minHeight: 372,
              }}
            >
              {templateLoading ? (
                <div style={{ fontSize: 13, color: 'var(--muted)' }}>
                  {t('configManage.loadingPlaceholder')}
                </div>
              ) : filteredTemplateList.length ? (
                <>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8, flex: 1 }}>
                    {pagedTemplateList.map((item) => (
                      <button
                        key={item.templateId}
                        type="button"
                        className="modal-option"
                        style={{
                          textAlign: 'left',
                          padding: '12px 14px',
                          border: '1px solid',
                          borderColor:
                            selectedTemplateId === item.templateId
                              ? 'var(--primary)'
                              : 'var(--panel-border)',
                          borderRadius: 12,
                          background:
                            selectedTemplateId === item.templateId
                              ? 'rgba(39, 94, 254, 0.08)'
                              : 'white',
                          cursor: 'pointer',
                        }}
                        onClick={() => setSelectedTemplateId(item.templateId)}
                      >
                        <div style={{ fontWeight: 600, marginBottom: 4 }}>{item.displayName}</div>
                        <div style={{ fontSize: 12, color: 'var(--muted)' }}>{item.templateFile}</div>
                      </button>
                    ))}
                  </div>
                  {filteredTemplateList.length > TEMPLATE_PAGE_SIZE ? (
                    <div style={{ paddingTop: 8 }}>
                      <Pagination
                        current={templatePage}
                        total={filteredTemplateList.length}
                        pageSize={TEMPLATE_PAGE_SIZE}
                        size="small"
                        onChange={handleTemplatePageChange}
                        showSizeChanger={false}
                      />
                    </div>
                  ) : null}
                </>
              ) : (
                <div style={{ fontSize: 13, color: 'var(--muted)' }}>
                  {t('configManage.templateNoData')}
                </div>
              )}
            </div>
          </aside>
          <section style={{ flex: 1, minWidth: 0 }}>
            {selectedTemplate ? (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
                <div>
                  <div style={{ fontSize: 18, fontWeight: 600, marginBottom: 6 }}>
                    {selectedTemplate.displayName}
                  </div>
                  <div style={{ fontSize: 13, color: 'var(--muted)' }}>
                    {t('configManage.templateConnect')} <code>{selectedTemplate.connect}</code>
                  </div>
                </div>

                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>
                    {t('configManage.requiredParams')}
                  </div>
                  {renderFieldTags(selectedTemplate.requiredFields)}
                </div>

                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>
                    {t('configManage.defaultParams')}
                  </div>
                  {renderFieldTags(selectedTemplate.insertedFields, 'muted')}
                </div>

                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>
                    {t('configManage.omittedAdvancedParams')}
                  </div>
                  {renderFieldTags(selectedTemplate.omittedFields, 'warning')}
                </div>

                {templatePreview?.warnings?.length ? (
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8, color: '#ad6800' }}>
                      {t('configManage.templateWarnings')}
                    </div>
                    <div
                      style={{
                        padding: '12px 14px',
                        borderRadius: 12,
                        background: 'rgba(250, 173, 20, 0.08)',
                        color: '#ad6800',
                        fontSize: 13,
                        whiteSpace: 'pre-wrap',
                      }}
                    >
                      {templatePreview.warnings.join('\n')}
                    </div>
                  </div>
                ) : null}

                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>
                    {t('configManage.templatePreview')}
                  </div>
                  <div style={{ fontSize: 12, color: 'var(--muted)', marginBottom: 8 }}>
                    {t('configManage.templateInstance', {
                      name: templatePreview?.instanceName || selectedTemplate.templateId,
                    })}
                  </div>
                  <pre
                    style={{
                      margin: 0,
                      padding: '14px 16px',
                      borderRadius: 12,
                      background: '#0b1020',
                      color: '#d5def5',
                      fontSize: 12,
                      lineHeight: 1.6,
                      overflow: 'auto',
                      minHeight: 180,
                    }}
                  >
                    {templatePreviewLoading
                      ? t('configManage.loadingPlaceholder')
                      : templatePreview?.snippet || ''}
                  </pre>
                </div>
              </div>
            ) : (
              <div style={{ fontSize: 13, color: 'var(--muted)' }}>
                {t('configManage.templateNoData')}
              </div>
            )}
          </section>
        </div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 12, marginTop: 20 }}>
          <button type="button" className="btn ghost" onClick={closeTemplateModal}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={handleApplyTemplate}
            disabled={!templatePreview?.content || templatePreviewLoading}
          >
            {t('common.confirm')}
          </button>
        </div>
      </Modal>

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

      <Modal
        title={t('configManage.addSourceConfig')}
        open={sourceAddModalVisible}
        onCancel={() => {
          setSourceAddModalVisible(false);
          setNewSourceFileName('');
        }}
        onOk={handleCreateSourceConfigFile}
        okText={t('common.confirm')}
        cancelText={t('common.cancel')}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ color: 'var(--muted)', fontSize: 13 }}>
            {t('configManage.fileNameRule')}
          </div>
          <Input
            placeholder={t('configManage.fileNamePlaceholder', { type: t('configManage.source') })}
            value={newSourceFileName}
            onChange={(event) => setNewSourceFileName(event.target.value)}
            onPressEnter={handleCreateSourceConfigFile}
          />
        </div>
      </Modal>

      <Modal
        title={t('configManage.addSinkConfig')}
        open={sinkAddModalVisible}
        onCancel={() => {
          setSinkAddModalVisible(false);
          setNewSinkFileName('');
        }}
        footer={null}
        width={460}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <div style={{ fontSize: 13, color: 'var(--muted)' }}>
            {t('configManage.addSinkConfigDesc')}
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: 8, fontWeight: 500 }}>
              {t('configManage.configFileName')}
            </label>
            <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--muted)' }}>
              {t('configManage.sinkFileNameRule')}
            </div>
            <Input
              value={newSinkFileName}
              onChange={(event) => setNewSinkFileName(event.target.value)}
              placeholder={t('configManage.sinkFileNamePlaceholder')}
              onPressEnter={handleCreateSinkConfigFile}
            />
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 12 }}>
            <button
              type="button"
              className="btn ghost"
              onClick={() => {
                setSinkAddModalVisible(false);
                setNewSinkFileName('');
              }}
            >
              {t('common.cancel')}
            </button>
            <button type="button" className="btn primary" onClick={handleCreateSinkConfigFile}>
              {t('common.confirm')}
            </button>
          </div>
        </div>
      </Modal>
    </>
  );
}

export default ConfigManagePage;
