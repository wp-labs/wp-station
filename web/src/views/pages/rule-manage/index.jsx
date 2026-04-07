import React, { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input, message, Modal, Pagination } from 'antd';
import { RuleType, fetchRuleFiles, fetchRuleConfig, validateRuleConfig, saveRuleConfig, createRuleFile, deleteRuleFile, saveKnowledgeRule } from '@/services/config';
import { wplCodeFormat, omlCodeFormat } from '@/services/debug';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';

const REPO_PAGE_SIZE = 15;
const KNOWLEDGE_PAGE_SIZE = 15;
const EMPTY_KNOWLEDGE_CONFIG = Object.freeze({
  config: '',
  createSql: '',
  insertSql: '',
  data: '',
});

const SINK_FILE_LABELS = Object.freeze({
  'business.d/sink.toml': '输出配置',
  'infra.d/error.toml': '异常数据',
  'infra.d/miss.toml': '未命中WPL数据',
  'infra.d/default.toml': '未命中OML数据',
  'infra.d/monitor.toml': '监控数据',
  'infra.d/residue.toml': '残留数据',
});

const SINK_FILE_ORDER = Object.freeze([
  'business.d/sink.toml',
  'infra.d/error.toml',
  'infra.d/miss.toml',
  'infra.d/default.toml',
  'infra.d/monitor.toml',
  'infra.d/residue.toml',
]);

const HIDDEN_SINK_FILES = new Set([
  'defaults.toml',
  'infra.d/intercept.toml',
]);

const WPL_PARSE_FILE = 'parse.wpl';
const WPL_SAMPLE_FILE = 'sample.dat';

const normalizeWplEntry = (value) => {
  if (value === undefined || value === null) {
    return '';
  }
  const trimmed = String(value).trim();
  if (!trimmed) {
    return '';
  }
  if (!trimmed.includes('/')) {
    return `${trimmed}/${WPL_PARSE_FILE}`;
  }
  const [rulePart, ...restParts] = trimmed.split('/');
  const rule = (rulePart || '').trim();
  const sub = (restParts.join('/') || '').trim() || WPL_PARSE_FILE;
  if (!rule) {
    return sub;
  }
  return `${rule}/${sub}`;
};

const normalizeWplList = (items) =>
  (Array.isArray(items) ? items : [])
    .map((item) => normalizeWplEntry(item))
    .filter((entry) => !!entry);

const getWplEntryParts = (entry) => {
  if (!entry) {
    return { rule: '', sub: '' };
  }
  const normalized = normalizeWplEntry(entry);
  if (!normalized) {
    return { rule: '', sub: '' };
  }
  const [rule, sub] = normalized.split('/');
  return {
    rule: (rule || '').trim(),
    sub: (sub || '').trim(),
  };
};

const isWplSampleEntry = (entry) => normalizeWplEntry(entry).endsWith(`/${WPL_SAMPLE_FILE}`);

const formatWplDisplayName = (entry) => {
  const { rule, sub } = getWplEntryParts(entry);
  if (!rule && !sub) {
    return '';
  }
  if (!rule) {
    return sub;
  }
  return `${rule}/${sub}`;
};

const buildWplTreeData = (items) => {
  const groups = new Map();
  (Array.isArray(items) ? items : []).forEach((entry) => {
    const normalized = normalizeWplEntry(entry);
    if (!normalized) {
      return;
    }
    const { rule, sub } = getWplEntryParts(normalized);
    if (!rule) {
      return;
    }
    const files = groups.get(rule) || [];
    files.push({
      value: normalized,
      label: sub || WPL_PARSE_FILE,
      isSample: sub === WPL_SAMPLE_FILE,
    });
    groups.set(rule, files);
  });

  return Array.from(groups.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([rule, files]) => ({
      rule,
      files: files.sort((a, b) => {
        if (a.isSample === b.isSample) {
          return a.label.localeCompare(b.label);
        }
        return a.isSample ? 1 : -1;
      }),
    }));
};

const getFirstWplEntry = (treeData) => treeData?.[0]?.files?.[0]?.value || '';

const getSinkFileLabel = (filePath) => {
  if (!filePath) {
    return '';
  }

  if (SINK_FILE_LABELS[filePath]) {
    return SINK_FILE_LABELS[filePath];
  }

  const fileName = filePath.split('/').pop() || filePath;
  return fileName.replace(/\.toml$/i, '');
};

/**
 * 规则配置管理页面
 * 功能：
 * 1. 显示和编辑各类规则配置（source/wpl/oml/knowledge/sink）
 * 2. 支持配置校验和保存
 * 对应原型：pages/views/rule-manage/source-config.html
 */
function RuleManagePage() {
  const { t } = useTranslation();
  
  // 定义 Modal 元数据的函数
  const getAddModalMeta = (type) => {
    const meta = {
      wpl: {
        title: t('ruleManage.addRuleFile'),
        placeholder: t('ruleManage.ruleFileNamePlaceholder'),
        tip: t('ruleManage.ruleFileTip'),
      },
      oml: {
        title: t('ruleManage.addEnrichmentRule'),
        placeholder: t('ruleManage.enrichmentRuleNamePlaceholder'),
        tip: t('ruleManage.enrichmentRuleTip'),
      },
      knowledge: {
        title: t('ruleManage.addDataset'),
        placeholder: t('ruleManage.datasetNamePlaceholder'),
        tip: t('ruleManage.datasetTip'),
      },
    };
    return meta[type] || {};
  };
  
  const [activeKey, setActiveKey] = useState(RuleType.SOURCE);
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  
  // wpl 配置的子文件列表
  const [wplFiles, setWplFiles] = useState([]);
  const [activeWplFile, setActiveWplFile] = useState('');
  const [localWplFiles, setLocalWplFiles] = useState([]);
  const [wplPage, setWplPage] = useState(1);
  const [wplTree, setWplTree] = useState([]);
  const [wplExpandedRules, setWplExpandedRules] = useState([]);
  
  // oml 配置的子文件列表
  const [omlFiles, setOmlFiles] = useState([]);
  const [activeOmlFile, setActiveOmlFile] = useState('');
  const [localOmlFiles, setLocalOmlFiles] = useState([]);
  const [omlPage, setOmlPage] = useState(1);
  const [wplTotal, setWplTotal] = useState(0);
  const [omlTotal, setOmlTotal] = useState(0);
  
  // knowledge 配置的数据集列表
  const [knowledgeDatasets, setKnowledgeDatasets] = useState([]);
  const [activeKnowledgeDataset, setActiveKnowledgeDataset] = useState('');
  const [localKnowledgeDatasets, setLocalKnowledgeDatasets] = useState([]);
  const [knowledgePage, setKnowledgePage] = useState(1);
  const [knowledgeTotal, setKnowledgeTotal] = useState(0);
  
  // knowledge 配置的4块内容
  const [knowledgeConfig, setKnowledgeConfig] = useState({ ...EMPTY_KNOWLEDGE_CONFIG });
  
  const [addModal, setAddModal] = useState({
    visible: false,
    type: null,
    title: '',
    placeholder: '',
    tip: '',
    value: '',
  });
  
  // 跟踪内容是否已修改
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const [originalContent, setOriginalContent] = useState('');
  const [originalKnowledgeConfig, setOriginalKnowledgeConfig] = useState({ ...EMPTY_KNOWLEDGE_CONFIG });

  // 跟踪 wpl/oml 列表当前悬停的文件名，用于展示删除按钮
  const [hoveredRepoFile, setHoveredRepoFile] = useState('');
  const [wplSearch, setWplSearch] = useState('');
  const [omlSearch, setOmlSearch] = useState('');
  const [knowledgeSearch, setKnowledgeSearch] = useState('');
  
  useEffect(() => {
    const initRuleLists = async () => {
      try {
        // 首次仅预加载 knowledge 列表，wpl/oml 在点击菜单时再懒加载
        const result = await fetchRuleFiles({
          type: 'knowledge',
          page: 1,
          pageSize: KNOWLEDGE_PAGE_SIZE,
        });
        const knowledgeList = Array.isArray(result?.items) ? result.items : [];
        setKnowledgeDatasets(knowledgeList);
        setKnowledgeTotal(result?.total || knowledgeList.length);
        setActiveKnowledgeDataset((prev) => prev || knowledgeList[0] || '');
        setKnowledgePage(result?.page || 1);
      } catch (error) {
        message.error('加载规则列表失败：' + error.message);
      }
    };
    initRuleLists();
  }, []);

  const totalWplPages = Math.max(1, Math.ceil(Math.max(wplTotal, 1) / REPO_PAGE_SIZE));
  const totalOmlPages = Math.max(1, Math.ceil(Math.max(omlTotal, 1) / REPO_PAGE_SIZE));
  const pagedOmlFiles = omlFiles; // 当前页数据由后端提供
  const totalKnowledgePages = Math.max(
    1,
    Math.ceil(Math.max(knowledgeTotal, 1) / KNOWLEDGE_PAGE_SIZE),
  );
  const pagedKnowledgeDatasets = knowledgeDatasets; // 当前页数据由后端提供
  
  // sink 配置的文件列表（从后端加载）
  const [sinkFiles, setSinkFiles] = useState([]);
  const [activeSinkFile, setActiveSinkFile] = useState('');

  /**
   * 加载配置内容
   */
  const loadConfig = async () => {
    // 调用服务层获取配置（使用对象参数）
    const options = { type: activeKey };
    if (activeKey === RuleType.WPL) {
      const targetFile = normalizeWplEntry(activeWplFile);
      if (!targetFile) {
        setContent('');
        return;
      }
      const targetRule = getWplEntryParts(targetFile).rule;
      if (targetRule && localWplFiles.includes(targetRule)) {
        setContent('');
        return;
      }
      options.file = targetFile;
    } else if (activeKey === RuleType.OML) {
      if (!activeOmlFile) {
        setContent('');
        return;
      }
      if (localOmlFiles.includes(activeOmlFile)) {
        setContent('');
        return;
      }
      options.file = activeOmlFile;
    } else if (activeKey === RuleType.KNOWLEDGE) {
      if (!activeKnowledgeDataset) {
        setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
        return;
      }
      options.file = activeKnowledgeDataset;
    } else if (activeKey === 'sink') {
      if (!activeSinkFile) {
        return;
      }
      options.file = activeSinkFile;
    }
    
    setLoading(true);
    try {
      const response = await fetchRuleConfig(options);
      if (activeKey === 'knowledge') {
        const newConfig = {
          config: response.config || '',
          createSql: response.createSql || '',
          insertSql: response.insertSql || '',
          data: response.data || '',
        };
        setKnowledgeConfig(newConfig);
        setOriginalKnowledgeConfig(newConfig);
        setHasUnsavedChanges(false);
      } else {
        const newContent = response.content || '';
        setContent(newContent);
        setOriginalContent(newContent);
        setHasUnsavedChanges(false);
      }
    } catch (error) {
      console.error('加载配置失败:', error);
      message.error(t('configManage.loadFailed', { message: error?.message || error }));
      // 加载失败时设置为空
      if (activeKey === 'knowledge') {
        setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
        setOriginalKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
      } else {
        setContent('');
        setOriginalContent('');
      }
    } finally {
      setLoading(false);
    }
  };

  const applyWplListState = useCallback(
    (rawItems, options = {}) => {
      const { preferredActive, preserveActive } = options;
      const normalizedList = normalizeWplList(rawItems);
      setWplFiles(normalizedList);
      const treeData = buildWplTreeData(normalizedList);
      setWplTree(treeData);

      if (!normalizedList.length) {
        setWplExpandedRules([]);
        setActiveWplFile('');
        return;
      }

      const normalizedPreferred = preferredActive ? normalizeWplEntry(preferredActive) : '';
      const normalizedActive = normalizeWplEntry(activeWplFile);

      let nextActive =
        normalizedPreferred && normalizedList.includes(normalizedPreferred)
          ? normalizedPreferred
          : null;
      if (!nextActive && preserveActive && normalizedList.includes(normalizedActive)) {
        nextActive = normalizedActive;
      }
      if (!nextActive) {
        nextActive = getFirstWplEntry(treeData) || normalizedList[0];
      }

      setActiveWplFile(nextActive);
      const { rule: activeRule } = getWplEntryParts(nextActive);

      setWplExpandedRules((prev) => {
        const availableRules = treeData.map((node) => node.rule);
        let nextExpanded =
          prev && prev.length
            ? prev.filter((rule) => availableRules.includes(rule))
            : availableRules;
        if (!nextExpanded.length) {
          nextExpanded = availableRules;
        }
        if (activeRule && !nextExpanded.includes(activeRule)) {
          nextExpanded = [...nextExpanded, activeRule];
        }
        return nextExpanded;
      });
    },
    [activeWplFile],
  );

  const toggleWplRule = (rule) => {
    setWplExpandedRules((prev) =>
      prev.includes(rule) ? prev.filter((item) => item !== rule) : [...prev, rule],
    );
  };

  const confirmDeleteWplRule = (ruleName) => {
    Modal.confirm({
      title: t('ruleManage.deleteConfirm'),
      content: t('ruleManage.deleteConfirmMessage', { filename: ruleName }),
      okText: t('common.delete'),
      okButtonProps: { danger: true },
      cancelText: t('common.cancel'),
      onOk: async () => {
        try {
          await deleteRuleFile({ type: 'wpl', file: ruleName });
          const refreshed = await fetchRuleFiles({
            type: 'wpl',
            page: wplPage,
            pageSize: REPO_PAGE_SIZE,
            keyword: wplSearch || undefined,
          });
          applyWplListState(refreshed?.items || [], { preserveActive: true });
          setWplTotal(
            refreshed?.total || (Array.isArray(refreshed?.items) ? refreshed.items.length : 0),
          );
          setWplPage(refreshed?.page || wplPage);
        } catch (error) {
          message.error(t('ruleManage.deleteFailed', { message: error.message }));
          throw error;
        }
      },
    });
  };

  /**
   * 懒加载 wpl/oml 规则文件列表
   */
  const loadRepoFilesIfNeeded = async (repoType) => {
    if (repoType === RuleType.WPL) {
      const result = await fetchRuleFiles({
        type: RuleType.WPL,
        page: 1,
        pageSize: REPO_PAGE_SIZE,
        keyword: wplSearch || undefined,
      });
      applyWplListState(result?.items || [], { preserveActive: true });
      setWplTotal(result?.total || (Array.isArray(result?.items) ? result.items.length : 0));
      setWplPage(result?.page || 1);
      return;
    }
    if (repoType === RuleType.OML) {
      const result = await fetchRuleFiles({
        type: RuleType.OML,
        page: 1,
        pageSize: REPO_PAGE_SIZE,
        keyword: omlSearch || undefined,
      });
      const files = Array.isArray(result?.items) ? result.items : [];
      setOmlFiles(files);
      setOmlTotal(result?.total || files.length);
      setActiveOmlFile((prev) => prev || files[0] || '');
      setOmlPage(result?.page || 1);
    }
  };

  // 当配置类型或子文件变化时重新加载
  useEffect(() => {
    loadConfig();
  }, [
    activeKey,
    activeWplFile,
    activeOmlFile,
    activeKnowledgeDataset,
    activeSinkFile,
    localWplFiles,
    localOmlFiles,
    // 注意：localKnowledgeDatasets 不在依赖中，因为 knowledge 不使用本地文件概念
  ]);

  /**
   * 处理页面切换
   */
  const handleNavigation = (newKey, additionalAction) => {
    // 当当前存在未保存修改时，切换到其他配置类型前弹出确认弹窗
    if (hasUnsavedChanges && newKey !== activeKey) {
      Modal.confirm({
        title: t('ruleManage.leaveConfirm'),
        content: t('ruleManage.leaveConfirmMessage'),
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: () => {
          setHasUnsavedChanges(false);
          setActiveKey(newKey);
          if (additionalAction) {
            additionalAction();
          }
        },
      });
      return;
    }

    setActiveKey(newKey);
    if (additionalAction) {
      additionalAction();
    }
  };

  /**
   * 监听内容变化
   */
  useEffect(() => {
    if (activeKey === 'knowledge') {
      const hasChanges = 
        knowledgeConfig.config !== originalKnowledgeConfig.config ||
        knowledgeConfig.createSql !== originalKnowledgeConfig.createSql ||
        knowledgeConfig.insertSql !== originalKnowledgeConfig.insertSql ||
        knowledgeConfig.data !== originalKnowledgeConfig.data;
      setHasUnsavedChanges(hasChanges);
    } else {
      setHasUnsavedChanges(content !== originalContent);
    }
  }, [content, originalContent, knowledgeConfig, originalKnowledgeConfig, activeKey]);

  /**
   * 处理配置校验
   * 校验配置语法是否正确
   */
  const handleValidate = async () => {
    const fileInfo = getCurrentFileInfo();
    if (!fileInfo.file) {
      message.warning(t('ruleManage.noFileToValidate'));
      return;
    }
    const currentContent = buildCurrentContent();
    try {
      // 调用服务层校验配置（使用对象参数）
      const response = await validateRuleConfig({ type: activeKey, file: fileInfo.file, content: currentContent });
      
      if (response.valid) {
        const warnings = response.warnings || 0;
        const statusColor = warnings === 0 ? '#52c41a' : '#faad14';
        const statusIcon = warnings === 0 ? '✓' : '⚠';
        const statusText = warnings === 0 ? t('ruleManage.validateSuccess') : t('ruleManage.validateWarning');
        const now = new Date().toLocaleString('zh-CN');
        const lineCount = response.lines ?? (currentContent ? currentContent.split('\n').length : 0);
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
                  <div style={{ fontSize: 13, color: '#666' }}>{t('ruleManage.conforms', { type: typeLabelMap[activeKey] || activeKey })}</div>
                </div>
              </div>
              <div style={{ background: '#fafafa', borderRadius: 8, padding: 16 }}>
                <table style={{ width: '100%', fontSize: 13, lineHeight: 2 }}>
                  <tbody>
                    <tr>
                      <td style={{ color: '#666', padding: '4px 0' }}>{t('ruleManage.fileName')}</td>
                      <td style={{ fontWeight: 500 }}>{fileInfo.display || fileInfo.file}</td>
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
        // 显示后端返回的错误信息
        Modal.error({
          title: t('ruleManage.validateFailed'),
          content: response.message || t('ruleManage.validateFailedMessage'),
        });
      }
    } catch (error) {
      message.error(t('ruleManage.validateFailed') + '：' + error.message);
    }
  };

  /**
   * 处理配置保存
   * 弹出确认框后保存配置
   */
  const handleSave = async () => {
    const fileInfo = getCurrentFileInfo();
    if (!fileInfo.file) {
      message.warning(t('ruleManage.noFileToSave'));
      return;
    }
    const currentMenuItem = menuItems.find((item) => item.key === activeKey);
    const configLabel = currentMenuItem?.label || activeKey;
    const currentContent = buildCurrentContent();

    try {
      if (activeKey === 'knowledge') {
        await saveKnowledgeRule({
          file: activeKnowledgeDataset,
          config: knowledgeConfig.config,
          createSql: knowledgeConfig.createSql,
          insertSql: knowledgeConfig.insertSql,
          data: knowledgeConfig.data,
        });
      } else {
        // 直接调用服务层保存配置（使用对象参数），不再弹出确认框
        await saveRuleConfig({
          type: activeKey,
          file: fileInfo.file,
          content: currentContent,
        });
      }

      // 保存成功后重置未保存状态
      if (activeKey === 'knowledge') {
        setOriginalKnowledgeConfig({ ...knowledgeConfig });
      } else {
        setOriginalContent(content);
      }
      setHasUnsavedChanges(false);

      // 精简版保存成功提示：只保留成功提示卡片
      Modal.info({
        icon: null,
        okText: t('common.confirm'),
        width: 420,
        title: t('ruleManage.saveSuccess'),
        content: (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              padding: 16,
              marginTop: 4,
              background: '#f6ffed',
              borderLeft: '3px solid #52c41a',
              borderRadius: 8,
            }}
          >
            <span style={{ fontSize: 28, color: '#52c41a' }}>✓</span>
            <div>
              <div style={{ fontSize: 16, fontWeight: 600, color: '#52c41a', marginBottom: 4 }}>
                {t('ruleManage.saveSuccess')}
              </div>
              <div style={{ fontSize: 13, color: '#666' }}>{t('ruleManage.saveSuccessMessage')}</div>
            </div>
          </div>
        ),
      });
    } catch (error) {
      message.error(t('ruleManage.saveFailed', { message: error.message }));
      throw error;
    }
  };

  /**
   * 处理代码格式化
   */
  const handleFormat = async () => {
    if (activeKey !== 'wpl' && activeKey !== 'oml') {
      return;
    }

    if (activeKey === RuleType.WPL && isWplSampleEntry(activeWplFile)) {
      message.info(t('ruleManage.sampleFileFormatDisabled'));
      return;
    }

    if (!content || content.trim() === '') {
      message.warning(t('simulateDebug.parseRule.formatError'));
      return;
    }

    try {
      let result;
      if (activeKey === 'wpl') {
        result = await wplCodeFormat(content);
      } else {
        result = await omlCodeFormat(content);
      }

      // 提取格式化后的代码
      const formattedCode = activeKey === 'wpl' ? result.wpl_code : result.oml_code;

      if (formattedCode && formattedCode !== content) {
        setContent(formattedCode);
        message.success(t('ruleManage.format'));
      } else {
        message.info(t('ruleManage.format'));
      }
    } catch (error) {
      message.error(t('simulateDebug.parseRule.formatError'));
    }
  };

  /**
   * 处理新增弹窗展示
   */
  const showAddModal = (modalType) => {
    const meta = getAddModalMeta(modalType);
    setAddModal({
      visible: true,
      type: modalType,
      title: meta.title || '新增',
      placeholder: meta.placeholder || '请输入名称',
      tip: meta.tip || '',
      value: '',
    });
  };

  const closeAddModal = () => {
    setAddModal((prev) => ({
      ...prev,
      visible: false,
      value: '',
      type: null,
    }));
  };

  /**
   * 处理 wpl/oml 规则文件新增（由弹窗确认触发）
   * repoType 仅支持 wpl 或 oml
   */
  const insertRepoFile = async (options) => {
    const { repoType, name } = options;
    const normalizedName = name.trim();
    if (!normalizedName) {
      message.warning(t('ruleManage.fileNameCannotBeEmpty'));
      return false;
    }
    if (repoType === 'wpl') {
      const exists = wplFiles.some((fileName) => getWplEntryParts(fileName).rule === normalizedName);
      if (exists) {
        message.warning(t('ruleManage.ruleFileExists'));
        return false;
      }
      await createRuleFile({ type: RuleType.WPL, file: normalizedName });
      await saveRuleConfig({ type: RuleType.WPL, file: normalizedName, content: '' });
      const refreshed = await fetchRuleFiles({
        type: RuleType.WPL,
        page: 1,
        pageSize: REPO_PAGE_SIZE,
        keyword: wplSearch || undefined,
      });
      applyWplListState(refreshed?.items || [], {
        preferredActive: `${normalizedName}/${WPL_PARSE_FILE}`,
      });
      setWplTotal(refreshed?.total || (Array.isArray(refreshed?.items) ? refreshed.items.length : 0));
      setLocalWplFiles((prev) => prev.filter((name) => name !== normalizedName));
      setContent('');
      setWplPage(refreshed?.page || 1);
      return true;
    }
    
    if (repoType === 'oml') {
      if (omlFiles.includes(normalizedName)) {
        message.warning(t('ruleManage.enrichmentRuleExists'));
        return false;
      }
      await createRuleFile({ type: RuleType.OML, file: normalizedName });
      await saveRuleConfig({ type: RuleType.OML, file: normalizedName, content: '' });
      const refreshed = await fetchRuleFiles({
        type: RuleType.OML,
        page: 1,
        pageSize: REPO_PAGE_SIZE,
        keyword: omlSearch || undefined,
      });
      const files = Array.isArray(refreshed?.items) ? refreshed.items : [];
      setOmlFiles(files);
      setOmlTotal(refreshed?.total || files.length);
      setLocalOmlFiles((prev) => prev.filter((name) => name !== normalizedName));
      setActiveOmlFile(normalizedName);
      setContent('');
      setOmlPage(1);
    }
    return true;
  };

  /**
   * 处理知识库数据集新增
   */
  const insertKnowledgeDataset = (name) => {
    const normalizedName = name.trim();
    if (!normalizedName) {
      message.warning(t('ruleManage.datasetNameCannotBeEmpty'));
      return false;
    }
    if (knowledgeDatasets.includes(normalizedName)) {
      message.warning(t('ruleManage.datasetExists'));
      return false;
    }

    // 创建知识库规则文件，实际写入数据库
    createRuleFile({ type: 'knowledge', file: normalizedName })
      .then(async () => {
        const refreshed = await fetchRuleFiles({
          type: 'knowledge',
          page: 1,
          pageSize: KNOWLEDGE_PAGE_SIZE,
          keyword: knowledgeSearch || undefined,
        });
        const datasets = Array.isArray(refreshed?.items) ? refreshed.items : [];
        setKnowledgeDatasets(datasets);
        setKnowledgeTotal(refreshed?.total || datasets.length);
        setActiveKnowledgeDataset(normalizedName);
        setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
        setKnowledgePage(1);
      })
      .catch((error) => {
        message.error('创建数据集失败：' + error.message);
      });

    return true;
  };

  const handleAddConfirm = async () => {
    if (!addModal.type) {
      closeAddModal();
      return;
    }
    const normalizedValue = (addModal.value || '').trim();
    if (!normalizedValue) {
      message.warning(t('ruleManage.nameCannotBeEmpty'));
      return;
    }
    let success = false;
    if (addModal.type === 'knowledge') {
      success = insertKnowledgeDataset(normalizedValue);
    } else {
      success = await insertRepoFile({ repoType: addModal.type, name: normalizedValue });
    }
    if (success) {
      closeAddModal();
    }
  };

  const menuItems = [
    { key: RuleType.SOURCE, label: t('ruleManage.sourceConfig') },
    { key: RuleType.WPL, label: t('ruleManage.wplConfig') },
    { key: RuleType.OML, label: t('ruleManage.omlConfig') },
    { key: RuleType.KNOWLEDGE, label: t('ruleManage.knowledgeConfig') },
    { key: RuleType.SINK, label: t('ruleManage.sinkSource') },
  ];
  const typeLabelMap = {
    source: 'Source',
    wpl: 'WPL',
    oml: 'OML',
    knowledge: 'Knowledge',
    sink: 'Sink',
  };

  // 获取页面标题（与旧版本一致）
  const getPageTitle = () => {
    const titles = {
      source: t('ruleManage.sourceConfig'),
      wpl: t('ruleManage.wplConfig'),
      oml: t('ruleManage.omlConfig'),
      knowledge: t('ruleManage.knowledgeConfig'),
      sink: t('ruleManage.sinkSource'),
    };
    return titles[activeKey] || t('ruleManage.title');
  };

  const getCurrentFileInfo = () => {
    if (activeKey === 'source') {
      return { file: 'wpsrc.toml', display: 'source 配置模板' };
    }
    if (activeKey === 'wpl') {
      const normalized = normalizeWplEntry(activeWplFile);
      return { file: normalized, display: formatWplDisplayName(normalized) };
    }
    if (activeKey === 'oml') {
      const displayName = activeOmlFile ? `${activeOmlFile}.oml` : '';
      return { file: activeOmlFile || '', display: displayName };
    }
    if (activeKey === 'knowledge') {
      const datasetName = activeKnowledgeDataset ? `${activeKnowledgeDataset}.dataset` : '';
      return { file: datasetName, display: datasetName };
    }
    if (activeKey === 'sink') {
      return { file: activeSinkFile, display: activeSinkFile };
    }
    return { file: '', display: '' };
  };

  const buildCurrentContent = () => {
    if (activeKey === 'knowledge') {
      return [
        knowledgeConfig.config ?? '',
        knowledgeConfig.createSql ?? '',
        knowledgeConfig.insertSql ?? '',
        knowledgeConfig.data ?? '',
      ]
        .filter((section) => section !== undefined && section !== null)
        .join('\n\n');
    }
    return content || '';
  };

  const currentFileInfo = getCurrentFileInfo();
  const codeEditorLanguage =
    activeKey === 'wpl' ? (isWplSampleEntry(activeWplFile) ? 'plain' : 'wpl') : 'oml';

  const displayedSinkFiles = React.useMemo(() => {
    if (!Array.isArray(sinkFiles) || sinkFiles.length === 0) {
      return [];
    }

    const sinkFileSet = new Set(
      sinkFiles.filter((file) => typeof file === 'string' && file && !HIDDEN_SINK_FILES.has(file)),
    );

    return SINK_FILE_ORDER.filter((file) => sinkFileSet.has(file));
  }, [sinkFiles]);

  return (
    <>
      {/* 左侧侧边栏 */}
      <aside className="side-nav" data-group="rule-manage">
        <h2>{t('ruleManage.title')}</h2>
        <button
          type="button"
          className={`side-item ${activeKey === 'source' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('source')}
        >
          {t('ruleManage.sourceConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'wpl' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('wpl', async () => {
            try {
              await loadRepoFilesIfNeeded('wpl');
              setLocalWplFiles([]);
            } catch (error) {
              message.error(t('ruleManage.loadWplFailed', { message: error.message }));
            }
          })}
        >
          {t('ruleManage.wplConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'oml' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('oml', async () => {
            try {
              await loadRepoFilesIfNeeded('oml');
              setActiveOmlFile((current) => current || omlFiles[0] || '');
              setOmlPage(1);
              setLocalOmlFiles([]);
            } catch (error) {
              message.error(t('ruleManage.loadOmlFailed', { message: error.message }));
            }
          })}
        >
          {t('ruleManage.omlConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'knowledge' ? 'is-active' : ''}`}
          onClick={() => handleNavigation('knowledge', async () => {
            try {
              const result = await fetchRuleFiles({
                type: RuleType.KNOWLEDGE,
                page: 1,
                pageSize: KNOWLEDGE_PAGE_SIZE,
                keyword: knowledgeSearch || undefined,
              });
              const datasets = Array.isArray(result?.items) ? result.items : [];
              setKnowledgeDatasets(datasets);
              setKnowledgeTotal(result?.total || datasets.length);
              const nextActive = datasets[0] || '';
              setActiveKnowledgeDataset(nextActive);
              if (!nextActive) {
                setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
              }
              setKnowledgePage(result?.page || 1);
            } catch (error) {
              message.error(t('ruleManage.loadKnowledgeFailed', { message: error.message }));
            }
          })}
        >
          {t('ruleManage.knowledgeConfig')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'sink' ? 'is-active' : ''}`}
          onClick={() =>
            handleNavigation('sink', async () => {
              try {
                const result = await fetchRuleFiles({ type: 'sink' });
                const files = Array.isArray(result?.items) ? result.items : [];
                setSinkFiles(files);
                setActiveSinkFile((prev) => prev || files[0] || '');
              } catch (error) {
                message.error('加载 sink 配置列表失败：' + error.message);
              }
            })
          }
        >
          {t('ruleManage.sinkSource')}
        </button>
      </aside>

      {/* 右侧配置内容区 */}
      <section className="page-panels">
        <article className="panel is-visible">
          <header className="panel-header">
            <h2>{getPageTitle()}</h2>
          </header>
          <section className="panel-body config-body">
            {/* source 配置 */}
            {activeKey === 'source' ? (
              <div className="single-config">
                <header className="single-config-header">
                  <span className="single-config-name" aria-hidden="true">
                    wpsrc.toml
                  </span>
                  <div className="single-config-actions">
                    <button type="button" className="btn tertiary" onClick={handleValidate}>
                      {t('ruleManage.validate')}
                    </button>
                    <button type="button" className="btn primary" onClick={handleSave}>
                      {t('ruleManage.save')}
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
            ) : activeKey === 'knowledge' ? (
              /* knowledge 配置显示 repo 布局（与 wpl/oml 一致） */
              <div className="repo-layout" data-repo="knowledge">
                <aside className="repo-tree" aria-label="知识库数据集列表">
                  <div className="repo-tree-header">
                    <h3>{t('ruleManage.datasets')}</h3>
                    <button
                      type="button"
                      className="btn ghost repo-add-btn"
                      onClick={() => showAddModal('knowledge')}
                    >
                      {t('ruleManage.add')}
                    </button>
                  </div>
                  <div style={{ padding: '4px 0 8px' }}>
                    <Input
                      size="small"
                      allowClear
                      placeholder={t('ruleManage.searchDatasets')}
                      value={knowledgeSearch}
                      onChange={(e) => {
                        const value = e.target.value;
                        setKnowledgeSearch(value);
                        const nextPage = 1;
                        setKnowledgePage(nextPage);
                        fetchRuleFiles({
                          type: RuleType.KNOWLEDGE,
                          page: nextPage,
                          pageSize: KNOWLEDGE_PAGE_SIZE,
                          keyword: value || undefined,
                        })
                          .then((result) => {
                            const datasets = Array.isArray(result?.items)
                              ? result.items
                              : [];
                            setKnowledgeDatasets(datasets);
                            setKnowledgeTotal(result?.total || datasets.length);
                            if (!datasets.includes(activeKnowledgeDataset)) {
                              const nextActive = datasets[0] || '';
                              setActiveKnowledgeDataset(nextActive);
                              if (!nextActive) {
                                setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
                              }
                            }
                          })
                          .catch((error) => {
                            message.error('加载知识库数据集列表失败：' + error.message);
                          });
                      }}
                    />
                  </div>
                  <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
                    {pagedKnowledgeDatasets.map((dataset) => (
                      <div
                        key={dataset}
                        className="repo-file-row"
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'space-between',
                          gap: 8,
                          position: 'relative',
                        }}
                        onMouseEnter={() => setHoveredRepoFile(dataset)}
                        onMouseLeave={() => setHoveredRepoFile('')}
                      >
                        <button
                          type="button"
                          className={`repo-file ${activeKnowledgeDataset === dataset ? 'is-active' : ''}`}
                          onClick={() => {
                            if (hasUnsavedChanges && dataset !== activeKnowledgeDataset) {
                              Modal.confirm({
                                title: t('ruleManage.leaveConfirm'),
                                content: t('ruleManage.leaveConfirmMessage'),
                                okText: t('common.confirm'),
                                cancelText: t('common.cancel'),
                                onOk: () => {
                                  setActiveKnowledgeDataset(dataset);
                                },
                              });
                            } else {
                              setActiveKnowledgeDataset(dataset);
                            }
                          }}
                          style={{ 
                            flex: 1, 
                            textAlign: 'left',
                            paddingRight: hoveredRepoFile === dataset ? '28px' : '8px',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                          }}
                        >
                          {dataset}
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
                            display: hoveredRepoFile === dataset ? 'inline-flex' : 'none',
                            alignItems: 'center',
                            justifyContent: 'center',
                          }}
                          onClick={(event) => {
                            event.stopPropagation();
                            const filename = dataset;
                            Modal.confirm({
                              title: t('ruleManage.deleteConfirm'),
                              content: t('ruleManage.deleteConfirmMessage', { filename }),
                              okText: t('common.delete'),
                              okButtonProps: { danger: true },
                              cancelText: t('common.cancel'),
                              onOk: async () => {
                                try {
                                  await deleteRuleFile({ type: 'knowledge', file: filename });
                                  // 计算删除后的页码
                                  const nextPage = Math.min(knowledgePage, totalKnowledgePages);
                                  const refreshed = await fetchRuleFiles({
                                    type: 'knowledge',
                                    page: nextPage,
                                    pageSize: KNOWLEDGE_PAGE_SIZE,
                                  });
                                  const datasets = Array.isArray(refreshed?.items)
                                    ? refreshed.items
                                    : [];
                                  setKnowledgeDatasets(datasets);
                                  setKnowledgeTotal(refreshed?.total || datasets.length);
                                  if (filename === activeKnowledgeDataset) {
                                    const nextActive = datasets[0] || '';
                                    setActiveKnowledgeDataset(nextActive);
                                    if (!nextActive) {
                                      setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
                                    }
                                  }
                                  setKnowledgePage(refreshed?.page || nextPage);
                                } catch (error) {
                                  message.error(t('ruleManage.deleteDatasetFailed', { message: error.message }));
                                }
                              },
                            });
                          }}
                        >
                          -
                        </button>
                      </div>
                    ))}
                  </div>
                  {knowledgeTotal > KNOWLEDGE_PAGE_SIZE ? (
                    <div className="repo-pagination">
                      <Pagination
                        size="small"
                        simple
                        current={knowledgePage}
                        pageSize={KNOWLEDGE_PAGE_SIZE}
                        total={knowledgeTotal}
                        onChange={(page) => {
                          setKnowledgePage(page);
                          fetchRuleFiles({
                            type: 'knowledge',
                            page,
                            pageSize: KNOWLEDGE_PAGE_SIZE,
                            keyword: knowledgeSearch || undefined,
                          })
                            .then((result) => {
                              const datasets = Array.isArray(result?.items)
                                ? result.items
                                : [];
                              setKnowledgeDatasets(datasets);
                              setKnowledgeTotal(result?.total || datasets.length);
                              if (!datasets.includes(activeKnowledgeDataset)) {
                                const nextActive = datasets[0] || '';
                                setActiveKnowledgeDataset(nextActive);
                                if (!nextActive) {
                                  setKnowledgeConfig({ ...EMPTY_KNOWLEDGE_CONFIG });
                                }
                              }
                            })
                            .catch((error) => {
                              message.error(t('ruleManage.loadKnowledgeFailed', { message: error.message }));
                            });
                        }}
                      />
                    </div>
                  ) : null}
                </aside>
                <div className="repo-content">
                  <section className={`knowledge-detail ${activeKnowledgeDataset ? 'is-visible' : ''}`}>
                    <div className="editor-toolbar">
                      <span className="editor-label">
                        {activeKnowledgeDataset ? t('ruleManage.datasetLabel', { name: activeKnowledgeDataset }) : t('ruleManage.datasets')}
                      </span>
                      <div className="editor-actions">
                        <button type="button" className="btn tertiary" onClick={handleValidate}>
                          {t('ruleManage.validate')}
                        </button>
                        <button type="button" className="btn primary" onClick={handleSave}>
                          {t('ruleManage.save')}
                        </button>
                      </div>
                    </div>
                    <div className="knowledge-block">
                      <span className="editor-subtitle">{t('ruleManage.configToml')}</span>
                      <CodeEditor
                        key="knowledge-config"
                        className="code-area code-area--large"
                        value={knowledgeConfig.config || ''}
                        onChange={(value) =>
                          setKnowledgeConfig((prev) => ({ ...prev, config: value }))
                        }
                        language="toml"
                        theme="vscodeDark"
                      />
                    </div>
                    <div className="knowledge-block">
                      <span className="editor-subtitle">{t('ruleManage.createSql')}</span>
                      <CodeEditor
                        key="knowledge-create-sql"
                        className="code-area code-area--large"
                        value={knowledgeConfig.createSql || ''}
                        onChange={(value) =>
                          setKnowledgeConfig((prev) => ({ ...prev, createSql: value }))
                        }
                        language="sql"
                        theme="vscodeDark"
                      />
                    </div>
                    <div className="knowledge-block">
                      <span className="editor-subtitle">{t('ruleManage.insertSql')}</span>
                      <CodeEditor
                        key="knowledge-insert-sql"
                        className="code-area code-area--large"
                        value={knowledgeConfig.insertSql || ''}
                        onChange={(value) =>
                          setKnowledgeConfig((prev) => ({ ...prev, insertSql: value }))
                        }
                        language="sql"
                        theme="vscodeDark"
                      />
                    </div>
                    <div className="knowledge-block">
                      <span className="editor-subtitle">
                        {activeKnowledgeDataset ? t('ruleManage.dataCsv', { name: activeKnowledgeDataset }) : t('ruleManage.datasetCsv')}
                      </span>
                      <CodeEditor
                        key="knowledge-data"
                        className="code-area code-area--large"
                        value={knowledgeConfig.data || ''}
                        onChange={(value) =>
                          setKnowledgeConfig((prev) => ({ ...prev, data: value }))
                        }
                        language="plain"
                        theme="vscodeDark"
                      />
                    </div>
                  </section>
                </div>
              </div>
        ) : (activeKey === 'wpl' || activeKey === 'oml') ? (
          /* wpl/oml 配置显示 repo 布局 */
          <div className="repo-layout" data-repo={activeKey}>
            <aside
              className="repo-tree"
              aria-label={`${activeKey === 'wpl' ? 'WPL' : 'OML'} 规则文件列表`}
            >
              <div className="repo-tree-header">
                <h3>{activeKey === 'wpl' ? t('ruleManage.ruleFiles') : t('ruleManage.enrichmentRules')}</h3>
                <button
                  type="button"
                  className="btn ghost repo-add-btn"
                  onClick={() => showAddModal(activeKey)}
                >
                  {t('ruleManage.add')}
                </button>
              </div>
              <div style={{ padding: '4px 0 8px' }}>
                <Input
                  size="small"
                  allowClear
                  placeholder={activeKey === 'wpl' ? t('ruleManage.searchRuleFiles') : t('ruleManage.searchEnrichmentRules')}
                  value={activeKey === 'wpl' ? wplSearch : omlSearch}
                  onChange={(e) => {
                    const value = e.target.value;
                  if (activeKey === 'wpl') {
                    setWplSearch(value);
                    const nextPage = 1;
                    setWplPage(nextPage);
                    fetchRuleFiles({
                        type: RuleType.WPL,
                        page: nextPage,
                        pageSize: REPO_PAGE_SIZE,
                        keyword: value || undefined,
                      })
                        .then((result) => {
                          applyWplListState(result?.items || [], { preserveActive: true });
                          setWplTotal(result?.total || (Array.isArray(result?.items) ? result.items.length : 0));
                        })
                        .catch((error) => {
                          message.error('加载 WPL 规则列表失败：' + error.message);
                        });
                    } else {
                      setOmlSearch(value);
                      const nextPage = 1;
                      setOmlPage(nextPage);
                      fetchRuleFiles({
                        type: RuleType.OML,
                        page: nextPage,
                        pageSize: REPO_PAGE_SIZE,
                        keyword: value || undefined,
                      })
                        .then((result) => {
                          const files = Array.isArray(result?.items) ? result.items : [];
                          setOmlFiles(files);
                          setOmlTotal(result?.total || files.length);
                          if (!files.includes(activeOmlFile)) {
                            const nextActive = files[0] || '';
                            setActiveOmlFile(nextActive);
                            if (!nextActive) {
                              setContent('');
                            }
                          }
                        })
                        .catch((error) => {
                          message.error('加载 OML 规则列表失败：' + error.message);
                        });
                    }
                  }}
                />
              </div>
              <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
                {activeKey === 'wpl'
                  ? wplTree.map((node) => {
                      const expanded = wplExpandedRules.includes(node.rule);
                      return (
                        <div
                          key={node.rule}
                          className="repo-file-group"
                          onMouseEnter={() => setHoveredRepoFile(node.rule)}
                          onMouseLeave={() => setHoveredRepoFile('')}
                        >
                          <div
                            style={{
                              display: 'flex',
                              alignItems: 'center',
                              gap: 8,
                              position: 'relative',
                            }}
                          >
                            <button
                              type="button"
                              className="repo-file repo-file--folder"
                              onClick={() => toggleWplRule(node.rule)}
                              style={{
                                flex: 1,
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'space-between',
                              }}
                            >
                              <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                <span aria-hidden="true">{expanded ? '📂' : '📁'}</span>
                                {node.rule}
                              </span>
                              <span style={{ fontSize: 12, color: '#999' }}>
                                {node.files.length}
                              </span>
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
                                display: hoveredRepoFile === node.rule ? 'inline-flex' : 'none',
                                alignItems: 'center',
                                justifyContent: 'center',
                              }}
                              onClick={(event) => {
                                event.stopPropagation();
                                confirmDeleteWplRule(node.rule);
                              }}
                            >
                              -
                            </button>
                          </div>
                          {expanded ? (
                            <div style={{ marginLeft: 24, marginTop: 4 }}>
                              {node.files.map((file) => (
                                <button
                                  key={file.value}
                                  type="button"
                                  className={`repo-file ${
                                    activeWplFile === file.value ? 'is-active' : ''
                                  }`}
                                  onClick={() => {
                                    if (hasUnsavedChanges && file.value !== activeWplFile) {
                                      Modal.confirm({
                                        title: t('ruleManage.leaveConfirm'),
                                        content: t('ruleManage.leaveConfirmMessage'),
                                        okText: t('common.confirm'),
                                        cancelText: t('common.cancel'),
                                        onOk: () => {
                                          setActiveWplFile(file.value);
                                        },
                                      });
                                    } else {
                                      setActiveWplFile(file.value);
                                    }
                                  }}
                                  style={{
                                    textAlign: 'left',
                                    paddingLeft: 24,
                                    position: 'relative',
                                  }}
                                >
                                  <span
                                    aria-hidden="true"
                                    style={{ position: 'absolute', left: 6, fontSize: 12 }}
                                  >
                                    •
                                  </span>
                                  {file.label}
                                </button>
                              ))}
                            </div>
                          ) : null}
                        </div>
                      );
                    })
                  : pagedOmlFiles.map((file) => (
                      <div
                        key={file}
                        className="repo-file-row"
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'space-between',
                          gap: 8,
                          position: 'relative',
                        }}
                        onMouseEnter={() => setHoveredRepoFile(file)}
                        onMouseLeave={() => setHoveredRepoFile('')}
                      >
                        <button
                          type="button"
                          className={`repo-file ${activeOmlFile === file ? 'is-active' : ''}`}
                          onClick={() => {
                            if (hasUnsavedChanges && file !== activeOmlFile) {
                              Modal.confirm({
                                title: t('ruleManage.leaveConfirm'),
                                content: t('ruleManage.leaveConfirmMessage'),
                                okText: t('common.confirm'),
                                cancelText: t('common.cancel'),
                                onOk: () => {
                                  setActiveOmlFile(file);
                                },
                              });
                            } else {
                              setActiveOmlFile(file);
                            }
                          }}
                          style={{
                            flex: 1,
                            textAlign: 'left',
                            paddingRight: hoveredRepoFile === file ? '28px' : '8px',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                          }}
                        >
                          {file}
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
                            display: hoveredRepoFile === file ? 'inline-flex' : 'none',
                            alignItems: 'center',
                            justifyContent: 'center',
                          }}
                          onClick={(event) => {
                            event.stopPropagation();
                            Modal.confirm({
                              title: t('ruleManage.deleteConfirm'),
                              content: t('ruleManage.deleteConfirmMessage', { filename: file }),
                              okText: t('common.delete'),
                              okButtonProps: { danger: true },
                              cancelText: t('common.cancel'),
                              onOk: async () => {
                                try {
                                  await deleteRuleFile({ type: RuleType.OML, file });
                                  const updated = omlFiles.filter((name) => name !== file);
                                  setOmlFiles(updated);
                                  if (activeOmlFile === file) {
                                    const next = updated[0] || '';
                                    setActiveOmlFile(next);
                                    if (!next) {
                                      setContent('');
                                    }
                                  }
                                } catch (error) {
                                  message.error(t('ruleManage.deleteFailed', { message: error.message }));
                                  throw error;
                                }
                              },
                            });
                          }}
                        >
                          -
                        </button>
                      </div>
                    ))}
              </div>
              {(activeKey === 'wpl' ? wplTotal : omlTotal) > REPO_PAGE_SIZE ? (
                <div className="repo-pagination">
                  <Pagination
                    size="small"
                    simple
                    current={activeKey === 'wpl' ? wplPage : omlPage}
                    pageSize={REPO_PAGE_SIZE}
                    total={activeKey === 'wpl' ? wplTotal : omlTotal}
                    onChange={(page) => {
                      if (activeKey === 'wpl') {
                        setWplPage(page);
                        fetchRuleFiles({
                          type: 'wpl',
                          page,
                          pageSize: REPO_PAGE_SIZE,
                          keyword: wplSearch || undefined,
                        })
                          .then((result) => {
                            applyWplListState(result?.items || [], { preserveActive: true });
                            setWplTotal(
                              result?.total || (Array.isArray(result?.items) ? result.items.length : 0),
                            );
                          })
                          .catch((error) => {
                            message.error(t('ruleManage.loadWplFailed', { message: error.message }));
                          });
                      } else {
                        setOmlPage(page);
                        fetchRuleFiles({
                          type: 'oml',
                          page,
                          pageSize: REPO_PAGE_SIZE,
                          keyword: omlSearch || undefined,
                        })
                          .then((result) => {
                            const files = Array.isArray(result?.items) ? result.items : [];
                            setOmlFiles(files);
                            setOmlTotal(result?.total || files.length);
                            if (!files.includes(activeOmlFile)) {
                              setActiveOmlFile(files[0] || '');
                              if (!files[0]) {
                                setContent('');
                              }
                            }
                          })
                          .catch((error) => {
                            message.error(t('ruleManage.loadOmlFailed', { message: error.message }));
                          });
                      }
                    }}
                  />
                </div>
              ) : null}
            </aside>

            <div className="repo-content">
              <div className="repo-toolbar">
                <div className="repo-path">
                  {activeKey === 'wpl'
                    ? activeWplFile
                      ? formatWplDisplayName(activeWplFile)
                      : t('ruleManage.noFileSelected')
                    : activeOmlFile
                      ? `${activeOmlFile}.oml`
                      : t('ruleManage.noFileSelected')}
                </div>
                <div className="editor-actions">
                  <button type="button" className="btn ghost" onClick={handleFormat}>
                    {t('ruleManage.format')}
                  </button>
                  <button type="button" className="btn tertiary" onClick={handleValidate}>
                    {t('ruleManage.validate')}
                  </button>
                  <button type="button" className="btn primary" onClick={handleSave}>
                    {t('ruleManage.save')}
                  </button>
                </div>
              </div>
              <div className="repo-view">
                <CodeEditor
                  className="code-area code-area--large repo-doc is-visible"
                  value={content}
                  onChange={(value) => setContent(value)}
                  language={codeEditorLanguage}
                  theme="vscodeDark"
                />
              </div>
            </div>
          </div>
        ) : activeKey === 'sink' ? (
          /* sink 配置显示语义化列表，内部文件路径保持不变 */
          <div className="repo-layout" data-repo="sink">
            <aside className="repo-tree" aria-label="sink 源配置文件列表">
              <h3>{t('ruleManage.configFiles')}</h3>
              <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
                {displayedSinkFiles.map((file) => (
                  <button
                    key={file}
                    type="button"
                    className={`repo-file ${activeSinkFile === file ? 'is-active' : ''}`}
                    onClick={() => {
                      if (hasUnsavedChanges && file !== activeSinkFile) {
                        Modal.confirm({
                          title: t('ruleManage.leaveConfirm'),
                          content: t('ruleManage.leaveConfirmMessage'),
                          okText: t('common.confirm'),
                          cancelText: t('common.cancel'),
                          onOk: () => {
                            setActiveSinkFile(file);
                          },
                        });
                      } else {
                        setActiveSinkFile(file);
                      }
                    }}
                    style={{ textAlign: 'left' }}
                  >
                    {getSinkFileLabel(file)}
                  </button>
                ))}
              </div>
            </aside>

            <div className="repo-content">
              <div className="repo-toolbar">
                <div className="repo-path">{activeSinkFile}</div>
                <div className="editor-actions">
                  <button type="button" className="btn tertiary" onClick={handleValidate}>
                    {t('ruleManage.validate')}
                  </button>
                  <button type="button" className="btn primary" onClick={handleSave}>
                    {t('ruleManage.save')}
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
      <Modal
        open={addModal.visible}
        title={addModal.title}
        okText={t('common.confirm')}
        cancelText={t('common.cancel')}
        onOk={handleAddConfirm}
        onCancel={closeAddModal}
        destroyOnClose
        centered
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <p style={{ margin: 0, color: '#5d6470' }}>{t('ruleManage.inputNameTip')}</p>
          <Input
            placeholder={addModal.placeholder}
            value={addModal.value}
            onChange={(e) =>
              setAddModal((prev) => ({
                ...prev,
                value: e.target.value,
              }))
            }
            onPressEnter={handleAddConfirm}
            allowClear
            autoFocus
          />
          {addModal.tip ? (
            <p style={{ margin: 0, fontSize: 12, color: '#999' }}>
              <span style={{ color: '#faad14', marginRight: 6 }}>⚠</span>
              {addModal.tip}
            </p>
          ) : null}
        </div>
      </Modal>
    </>
  );
}

export default RuleManagePage;
