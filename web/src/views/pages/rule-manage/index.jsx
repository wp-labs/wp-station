import React, { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input, message, Modal, Pagination } from 'antd';
import {
  RuleType,
  fetchRuleFiles,
  fetchRuleConfig,
  validateRuleConfig,
  saveRuleConfig,
  createRuleFile,
  deleteRuleFile,
  saveKnowledgeRule,
  fetchKnowdbConfig,
  saveKnowdbConfig,
} from '@/services/config';
import { wplCodeFormat, omlCodeFormat } from '@/services/debug';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';

const WPL_PAGE_SIZE = 14;
const WPL_FETCH_PAGE_SIZE = 50;
const OML_FOLDER_PAGE_SIZE = 14;
const OML_FETCH_PAGE_SIZE = 50;
const KNOWLEDGE_PAGE_SIZE = 15;
const EMPTY_KNOWLEDGE_DATASET = Object.freeze({
  createSql: '',
  insertSql: '',
  data: '',
});

const KNOWLEDGE_CONFIG_FILE = 'knowdb.toml';

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

const normalizeWplList = (items) => {
  const deduped = new Set();
  (Array.isArray(items) ? items : []).forEach((item) => {
    const entry = normalizeWplEntry(item);
    if (entry) {
      deduped.add(entry);
    }
  });
  return Array.from(deduped);
};

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

// 规则树分页按一级文件夹计算，每页容量对应用户实际看到的文件夹数量。
const buildRuleTreePages = (treeData, pageSize) => {
  const normalizedPageSize = pageSize > 0 ? pageSize : WPL_PAGE_SIZE;
  const nodes = Array.isArray(treeData) ? treeData : [];
  const pages = [];

  for (let index = 0; index < nodes.length; index += normalizedPageSize) {
    pages.push(nodes.slice(index, index + normalizedPageSize));
  }

  return pages;
};

const getWplEntriesFromTreeData = (treeData) =>
  (Array.isArray(treeData) ? treeData : []).flatMap((node) =>
    (Array.isArray(node.files) ? node.files : [])
      .map((item) => item.value)
      .filter(Boolean),
  );

const findWplPageByFile = (pagePlans, file) =>
  (Array.isArray(pagePlans) ? pagePlans : []).findIndex((page) =>
    page.some((node) => node.files.some((item) => item.value === file)),
  );

const findWplRuleForFile = (treeData, file) => {
  if (!file) {
    return '';
  }
  for (const node of treeData || []) {
    if (node.files.some((item) => item.value === file)) {
      return node.rule;
    }
  }
  return '';
};

const buildOmlTreeData = (items) => {
  const groups = new Map();
  (Array.isArray(items) ? items : []).forEach((entry) => {
    if (typeof entry !== 'string') {
      return;
    }
    const normalized = entry.trim();
    if (!normalized) {
      return;
    }
    const parts = normalized.split('/').filter(Boolean);
    if (!parts.length) {
      return;
    }
    const [group, ...rest] = parts;
    const files = groups.get(group) || [];
    files.push({
      value: normalized,
      label: rest.length ? rest.join('/') : 'adm.oml',
    });
    groups.set(group, files);
  });

  return Array.from(groups.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([group, files]) => ({
      group,
      files: files.sort((a, b) => a.label.localeCompare(b.label)),
    }));
};

const normalizeOmlList = (items) => {
  const deduped = new Set();
  (Array.isArray(items) ? items : []).forEach((item) => {
    if (typeof item !== 'string') {
      return;
    }
    const normalized = item.trim();
    if (normalized) {
      deduped.add(normalized);
    }
  });
  return Array.from(deduped);
};

const buildOmlTreePages = (treeData, pageSize) => {
  return buildRuleTreePages(treeData, pageSize > 0 ? pageSize : OML_FOLDER_PAGE_SIZE);
};

const getOmlEntriesFromTreeData = (treeData) =>
  (Array.isArray(treeData) ? treeData : []).flatMap((node) =>
    (Array.isArray(node.files) ? node.files : [])
      .map((item) => item.value)
      .filter(Boolean),
  );

const findOmlPageByFile = (pagePlans, file) =>
  (Array.isArray(pagePlans) ? pagePlans : []).findIndex((page) =>
    page.some((node) => node.files.some((item) => item.value === file)),
  );

const getFirstOmlEntry = (treeData) => treeData?.[0]?.files?.[0]?.value || '';

const findOmlGroupForFile = (treeData, file) => {
  if (!file) {
    return '';
  }
  for (const node of treeData || []) {
    if (node.files.some((item) => item.value === file)) {
      return node.group;
    }
  }
  return '';
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
  const [allWplFiles, setAllWplFiles] = useState([]);
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
  const omlPageSize = OML_FOLDER_PAGE_SIZE;
  const [wplTotal, setWplTotal] = useState(0);
  const [omlTotal, setOmlTotal] = useState(0);
  const [omlTree, setOmlTree] = useState([]);
  const [omlExpandedGroups, setOmlExpandedGroups] = useState([]);
  
  // knowledge 配置的数据集列表
  const [knowledgeDatasets, setKnowledgeDatasets] = useState([]);
  const [activeKnowledgeDataset, setActiveKnowledgeDataset] = useState('');
  const [localKnowledgeDatasets, setLocalKnowledgeDatasets] = useState([]);
  const [knowledgePage, setKnowledgePage] = useState(1);
  const [knowledgeTotal, setKnowledgeTotal] = useState(0);
  
  // knowledge 数据集内容 & 全局 knowdb
  const [knowledgeDatasetConfig, setKnowledgeDatasetConfig] = useState({
    ...EMPTY_KNOWLEDGE_DATASET,
  });
  const [originalKnowledgeDatasetConfig, setOriginalKnowledgeDatasetConfig] = useState({
    ...EMPTY_KNOWLEDGE_DATASET,
  });
  const [knowdbConfig, setKnowdbConfig] = useState('');
  const [originalKnowdbConfig, setOriginalKnowdbConfig] = useState('');
  
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
  const isKnowdbSelected = activeKey === RuleType.KNOWLEDGE && activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE;

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
        const normalizedList = knowledgeList.filter((item) => item !== KNOWLEDGE_CONFIG_FILE);
        setKnowledgeDatasets(normalizedList);
        setKnowledgeTotal(result?.total || normalizedList.length);
        setActiveKnowledgeDataset((prev) => prev || KNOWLEDGE_CONFIG_FILE);
        setKnowledgePage(result?.page || 1);
      } catch (error) {
        message.error('加载规则列表失败：' + error.message);
      }

      try {
        const resp = await fetchKnowdbConfig();
        const content = resp?.content || '';
        setKnowdbConfig(content);
        setOriginalKnowdbConfig(content);
      } catch (error) {
        message.warn('加载 knowdb 配置失败：' + (error.message || ''));
      }
    };
    initRuleLists();
  }, []);

  const totalKnowledgePages = Math.max(
    1,
    Math.ceil(Math.max(knowledgeTotal, 1) / KNOWLEDGE_PAGE_SIZE),
  );
  const pagedKnowledgeDatasets = knowledgeDatasets; // 当前页数据由后端提供
  const knowledgeListForDisplay = React.useMemo(() => {
    const datasets = Array.isArray(pagedKnowledgeDatasets) ? pagedKnowledgeDatasets : [];
    return [KNOWLEDGE_CONFIG_FILE, ...datasets];
  }, [pagedKnowledgeDatasets]);
  
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
        setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
        return;
      }
      if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
        setLoading(true);
        try {
          const resp = await fetchKnowdbConfig();
          const content = resp?.content || '';
          setKnowdbConfig(content);
          setOriginalKnowdbConfig(content);
          setHasUnsavedChanges(false);
        } catch (error) {
          message.error('加载 knowdb 配置失败：' + (error.message || ''));
        } finally {
          setLoading(false);
        }
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
        if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
          const content = response?.content || '';
          setKnowdbConfig(content);
          setOriginalKnowdbConfig(content);
        } else {
          const newConfig = {
            createSql: response.createSql || '',
            insertSql: response.insertSql || '',
            data: response.data || '',
          };
          setKnowledgeDatasetConfig(newConfig);
          setOriginalKnowledgeDatasetConfig(newConfig);
          if (typeof response.config === 'string' && response.config !== '') {
            setKnowdbConfig(response.config);
            setOriginalKnowdbConfig(response.config);
          }
        }
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
        if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
          setKnowdbConfig('');
          setOriginalKnowdbConfig('');
        } else {
          setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
          setOriginalKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
        }
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
      const { preferredActive, preserveActive, page } = options;
      const normalizedList = normalizeWplList(rawItems);
      setAllWplFiles(normalizedList);
      const treeData = buildWplTreeData(normalizedList);
      const pagePlans = buildRuleTreePages(treeData, WPL_PAGE_SIZE);
      const totalPages = pagePlans.length;
      setWplTotal(treeData.length);

      if (!normalizedList.length) {
        setWplTree([]);
        setWplPage(1);
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

      let nextPage = typeof page === 'number' && page > 0 ? page : null;
      if (!nextPage && nextActive) {
        const preferredPageIndex = findWplPageByFile(pagePlans, nextActive);
        if (preferredPageIndex >= 0) {
          nextPage = preferredPageIndex + 1;
        }
      }
      const currentPage = Math.min(Math.max(nextPage || 1, 1), totalPages);
      const currentTreeData = pagePlans[currentPage - 1] || [];
      const currentPageFiles = getWplEntriesFromTreeData(currentTreeData);

      if (!currentPageFiles.includes(nextActive)) {
        nextActive = getFirstWplEntry(currentTreeData) || currentPageFiles[0] || '';
      }

      setWplTree(currentTreeData);
      setWplPage(currentPage);
      setActiveWplFile(nextActive);
      const activeRule = findWplRuleForFile(currentTreeData, nextActive);

      const expandedRules = currentTreeData.map((node) => node.rule);
      setWplExpandedRules(
        activeRule && !expandedRules.includes(activeRule)
          ? [...expandedRules, activeRule]
          : expandedRules,
      );
    },
    [activeWplFile],
  );

  const applyOmlListState = useCallback(
    (rawItems, options = {}) => {
      const { preferredActive, preserveActive, page } = options;
      const normalizedList = normalizeOmlList(rawItems);
      const treeData = buildOmlTreeData(normalizedList);
      const pagePlans = buildOmlTreePages(treeData, omlPageSize);
      const totalPages = pagePlans.length;

      setOmlFiles(normalizedList);
      setOmlTotal(treeData.length);

      if (!normalizedList.length) {
        setOmlTree([]);
        setOmlPage(1);
        setOmlExpandedGroups([]);
        setActiveOmlFile('');
        return;
      }

      const normalizedPreferred =
        preferredActive && normalizedList.includes(preferredActive) ? preferredActive : '';
      const normalizedActive = normalizedList.includes(activeOmlFile) ? activeOmlFile : '';

      let nextPage = typeof page === 'number' && page > 0 ? page : null;
      if (!nextPage && normalizedPreferred) {
        const preferredPageIndex = findOmlPageByFile(pagePlans, normalizedPreferred);
        if (preferredPageIndex >= 0) {
          nextPage = preferredPageIndex + 1;
        }
      }
      if (!nextPage && preserveActive && normalizedActive) {
        const activePageIndex = findOmlPageByFile(pagePlans, normalizedActive);
        if (activePageIndex >= 0) {
          nextPage = activePageIndex + 1;
        }
      }
      const currentPage = Math.min(Math.max(nextPage || 1, 1), totalPages);
      const currentTreeData = pagePlans[currentPage - 1] || [];
      const currentPageFiles = getOmlEntriesFromTreeData(currentTreeData);

      let nextActive = currentPageFiles.includes(normalizedPreferred) ? normalizedPreferred : '';
      if (!nextActive && preserveActive && currentPageFiles.includes(normalizedActive)) {
        nextActive = normalizedActive;
      }
      if (!nextActive) {
        nextActive = getFirstOmlEntry(currentTreeData) || currentPageFiles[0] || '';
      }

      setOmlTree(currentTreeData);
      setOmlPage(currentPage);
      setActiveOmlFile(nextActive);
      const expandedGroups = currentTreeData.map((node) => node.group);
      const activeGroup = findOmlGroupForFile(currentTreeData, nextActive);
      setOmlExpandedGroups(
        activeGroup && !expandedGroups.includes(activeGroup)
          ? [...expandedGroups, activeGroup]
          : expandedGroups,
      );
    },
    [activeOmlFile, omlPageSize],
  );

  const fetchAllRuleFiles = useCallback(async (type, keyword, fetchPageSize) => {
    const normalizedKeyword =
      typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;
    const collected = [];
    const seen = new Set();
    let currentPage = 1;

    while (true) {
      const result = await fetchRuleFiles({
        type,
        page: currentPage,
        pageSize: fetchPageSize,
        keyword: normalizedKeyword,
      });
      const rawItems = Array.isArray(result?.items) ? result.items : [];
      const items =
        type === RuleType.WPL
          ? normalizeWplList(rawItems)
          : normalizeOmlList(rawItems);
      const pageSize =
        typeof result?.pageSize === 'number' && result.pageSize > 0
          ? result.pageSize
          : fetchPageSize;
      const total = typeof result?.total === 'number' ? result.total : 0;
      const totalPages = total > 0 ? Math.ceil(total / pageSize) : 0;

      items.forEach((item) => {
        if (!seen.has(item)) {
          seen.add(item);
          collected.push(item);
        }
      });

      if (
        totalPages
          ? currentPage >= totalPages
          : !rawItems.length || rawItems.length < pageSize
      ) {
        break;
      }
      currentPage += 1;
    }

    return collected;
  }, []);

  const refreshWplFiles = useCallback(
    async (options = {}) => {
      const {
        keyword = wplSearch,
        page,
        preferredActive,
        preserveActive = true,
      } = options;
      const files = await fetchAllRuleFiles(RuleType.WPL, keyword, WPL_FETCH_PAGE_SIZE);
      applyWplListState(files, { page, preferredActive, preserveActive });
      return files;
    },
    [applyWplListState, fetchAllRuleFiles, wplSearch],
  );

  const refreshOmlFiles = useCallback(
    async (options = {}) => {
      const {
        keyword = omlSearch,
        page,
        preferredActive,
        preserveActive = true,
      } = options;
      const files = await fetchAllRuleFiles(RuleType.OML, keyword, OML_FETCH_PAGE_SIZE);
      applyOmlListState(files, { page, preferredActive, preserveActive });
      return files;
    },
    [applyOmlListState, fetchAllRuleFiles, omlSearch],
  );

  const toggleWplRule = (rule) => {
    setWplExpandedRules((prev) =>
      prev.includes(rule) ? prev.filter((item) => item !== rule) : [...prev, rule],
    );
  };

  const toggleOmlGroup = (group) => {
    setOmlExpandedGroups((prev) =>
      prev.includes(group) ? prev.filter((item) => item !== group) : [...prev, group],
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
          await refreshWplFiles({ page: wplPage, preserveActive: true });
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
      await refreshWplFiles({ preserveActive: true, page: wplPage });
      return;
    }
    if (repoType === RuleType.OML) {
      await refreshOmlFiles({ preserveActive: true, page: omlPage });
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
    // 注意：localWplFiles/localOmlFiles 不放入依赖，避免一次加载导致重复请求
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
      if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
        setHasUnsavedChanges(knowdbConfig !== originalKnowdbConfig);
      } else {
        const hasChanges =
          knowledgeDatasetConfig.createSql !== originalKnowledgeDatasetConfig.createSql ||
          knowledgeDatasetConfig.insertSql !== originalKnowledgeDatasetConfig.insertSql ||
          knowledgeDatasetConfig.data !== originalKnowledgeDatasetConfig.data;
        setHasUnsavedChanges(hasChanges);
      }
    } else {
      setHasUnsavedChanges(content !== originalContent);
    }
  }, [
    content,
    originalContent,
    knowledgeDatasetConfig,
    originalKnowledgeDatasetConfig,
    knowdbConfig,
    originalKnowdbConfig,
    activeKey,
    activeKnowledgeDataset,
  ]);

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
    if (activeKey === 'knowledge' && activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
      message.info('knowdb.toml 不需要执行独立校验');
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
        if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
          await saveKnowdbConfig(knowdbConfig);
          setOriginalKnowdbConfig(knowdbConfig);
        } else {
          await saveKnowledgeRule({
            file: activeKnowledgeDataset,
            config: knowdbConfig,
            createSql: knowledgeDatasetConfig.createSql,
            insertSql: knowledgeDatasetConfig.insertSql,
            data: knowledgeDatasetConfig.data,
          });
          setOriginalKnowledgeDatasetConfig({ ...knowledgeDatasetConfig });
        }
      } else {
        // 直接调用服务层保存配置（使用对象参数），不再弹出确认框
        await saveRuleConfig({
          type: activeKey,
          file: fileInfo.file,
          content: currentContent,
        });
      }

      // 保存成功后重置未保存状态
      if (activeKey !== 'knowledge') {
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
      const exists = allWplFiles.some(
        (fileName) => getWplEntryParts(fileName).rule === normalizedName,
      );
      if (exists) {
        message.warning(t('ruleManage.ruleFileExists'));
        return false;
      }
      await createRuleFile({ type: RuleType.WPL, file: normalizedName });
      await saveRuleConfig({ type: RuleType.WPL, file: normalizedName, content: '' });
      await refreshWplFiles({
        preferredActive: `${normalizedName}/${WPL_PARSE_FILE}`,
        preserveActive: false,
      });
      setLocalWplFiles((prev) => prev.filter((name) => name !== normalizedName));
      setContent('');
      return true;
    }
    
    if (repoType === 'oml') {
      if (omlFiles.includes(normalizedName)) {
        message.warning(t('ruleManage.enrichmentRuleExists'));
        return false;
      }
      await createRuleFile({ type: RuleType.OML, file: normalizedName });
      await saveRuleConfig({ type: RuleType.OML, file: normalizedName, content: '' });
      await refreshOmlFiles({
        preferredActive: normalizedName,
        preserveActive: false,
      });
      setLocalOmlFiles((prev) => prev.filter((name) => name !== normalizedName));
      setContent('');
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
        const normalizedDatasets = datasets.filter((item) => item !== KNOWLEDGE_CONFIG_FILE);
        setKnowledgeDatasets(normalizedDatasets);
        setKnowledgeTotal(refreshed?.total || normalizedDatasets.length);
        setActiveKnowledgeDataset(normalizedName);
        setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
        setOriginalKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
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
      if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
        return { file: KNOWLEDGE_CONFIG_FILE, display: KNOWLEDGE_CONFIG_FILE };
      }
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
      if (activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE) {
        return knowdbConfig || '';
      }
      return [
        knowledgeDatasetConfig.createSql ?? '',
        knowledgeDatasetConfig.insertSql ?? '',
        knowledgeDatasetConfig.data ?? '',
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

    return sinkFiles
      .map((item) => {
        if (!item || !item.file) {
          return null;
        }
        const label =
          typeof item.displayName === 'string' && item.displayName.trim()
            ? item.displayName.trim()
            : item.file;
        return {
          file: item.file,
          displayName: label,
        };
      })
      .filter(Boolean);
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
              const normalizedDatasets = datasets.filter((item) => item !== KNOWLEDGE_CONFIG_FILE);
              setKnowledgeDatasets(normalizedDatasets);
              setKnowledgeTotal(result?.total || normalizedDatasets.length);
              const nextActive = normalizedDatasets.includes(activeKnowledgeDataset)
                ? activeKnowledgeDataset
                : KNOWLEDGE_CONFIG_FILE;
              setActiveKnowledgeDataset(nextActive);
              if (nextActive !== KNOWLEDGE_CONFIG_FILE && !normalizedDatasets.length) {
                setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
              }
              setKnowledgePage(result?.page || 1);
              const knowdbResp = await fetchKnowdbConfig();
              const content = knowdbResp?.content || '';
              setKnowdbConfig(content);
              setOriginalKnowdbConfig(content);
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
                setActiveSinkFile((prev) => prev || files[0]?.file || '');
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
                            const normalized = datasets.filter(
                              (item) => item !== KNOWLEDGE_CONFIG_FILE,
                            );
                            setKnowledgeDatasets(normalized);
                            setKnowledgeTotal(result?.total || normalized.length);
                            if (
                              activeKnowledgeDataset !== KNOWLEDGE_CONFIG_FILE &&
                              !normalized.includes(activeKnowledgeDataset)
                            ) {
                              const nextActive = normalized[0] || KNOWLEDGE_CONFIG_FILE;
                              setActiveKnowledgeDataset(nextActive);
                              if (nextActive === KNOWLEDGE_CONFIG_FILE) {
                                setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
                                setOriginalKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
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
                    {knowledgeListForDisplay.map((dataset) => {
                      const isConfigEntry = dataset === KNOWLEDGE_CONFIG_FILE;
                      return (
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
                              paddingRight:
                                hoveredRepoFile === dataset && !isConfigEntry ? '28px' : '8px',
                              overflow: 'hidden',
                              textOverflow: 'ellipsis',
                              whiteSpace: 'nowrap',
                            }}
                          >
                            {isConfigEntry ? KNOWLEDGE_CONFIG_FILE : dataset}
                          </button>
                          {!isConfigEntry && (
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
                                      const nextPage = Math.min(
                                        knowledgePage,
                                        totalKnowledgePages,
                                      );
                                      const refreshed = await fetchRuleFiles({
                                        type: 'knowledge',
                                        page: nextPage,
                                        pageSize: KNOWLEDGE_PAGE_SIZE,
                                      });
                                      const datasets = Array.isArray(refreshed?.items)
                                        ? refreshed.items
                                        : [];
                                      const normalized = datasets.filter(
                                        (item) => item !== KNOWLEDGE_CONFIG_FILE,
                                      );
                                      setKnowledgeDatasets(normalized);
                                      setKnowledgeTotal(refreshed?.total || normalized.length);
                                      if (filename === activeKnowledgeDataset) {
                                        const nextActive =
                                          normalized[0] || KNOWLEDGE_CONFIG_FILE;
                                        setActiveKnowledgeDataset(nextActive);
                                        if (nextActive !== KNOWLEDGE_CONFIG_FILE && !normalized.length) {
                                          setKnowledgeDatasetConfig({
                                            ...EMPTY_KNOWLEDGE_DATASET,
                                          });
                                          setOriginalKnowledgeDatasetConfig({
                                            ...EMPTY_KNOWLEDGE_DATASET,
                                          });
                                        }
                                      }
                                      setKnowledgePage(refreshed?.page || nextPage);
                                    } catch (error) {
                                      message.error(
                                        t('ruleManage.deleteDatasetFailed', {
                                          message: error.message,
                                        }),
                                      );
                                    }
                                  },
                                });
                              }}
                            >
                              -
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {knowledgeTotal > KNOWLEDGE_PAGE_SIZE ? (
                    <div className="repo-pagination">
                      <Pagination
                        size="small"
                        simple
                        current={knowledgePage}
                        pageSize={KNOWLEDGE_PAGE_SIZE}
                        total={knowledgeTotal}
                        showSizeChanger={false}
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
                              const normalized = datasets.filter(
                                (item) => item !== KNOWLEDGE_CONFIG_FILE,
                              );
                              setKnowledgeDatasets(normalized);
                              setKnowledgeTotal(result?.total || normalized.length);
                              if (
                                activeKnowledgeDataset !== KNOWLEDGE_CONFIG_FILE &&
                                !normalized.includes(activeKnowledgeDataset)
                              ) {
                                const nextActive = normalized[0] || KNOWLEDGE_CONFIG_FILE;
                                setActiveKnowledgeDataset(nextActive);
                                if (nextActive === KNOWLEDGE_CONFIG_FILE) {
                                  setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
                                  setOriginalKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
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
                        {activeKnowledgeDataset
                          ? activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE
                            ? KNOWLEDGE_CONFIG_FILE
                            : t('ruleManage.datasetLabel', { name: activeKnowledgeDataset })
                          : t('ruleManage.datasets')}
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
                    {activeKnowledgeDataset === KNOWLEDGE_CONFIG_FILE ? (
                      <div className="knowledge-block">
                        <span className="editor-subtitle">knowdb.toml</span>
                        <CodeEditor
                          key="knowledge-config"
                          className="code-area code-area--large"
                          value={knowdbConfig}
                          onChange={(value) => setKnowdbConfig(value)}
                          language="toml"
                          theme="vscodeDark"
                        />
                      </div>
                    ) : (
                      <>
                        <div className="knowledge-block">
                          <span className="editor-subtitle">{t('ruleManage.createSql')}</span>
                          <CodeEditor
                            key="knowledge-create-sql"
                            className="code-area code-area--large"
                            value={knowledgeDatasetConfig.createSql || ''}
                            onChange={(value) =>
                              setKnowledgeDatasetConfig((prev) => ({ ...prev, createSql: value }))
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
                            value={knowledgeDatasetConfig.insertSql || ''}
                            onChange={(value) =>
                              setKnowledgeDatasetConfig((prev) => ({ ...prev, insertSql: value }))
                            }
                            language="sql"
                            theme="vscodeDark"
                          />
                        </div>
                        <div className="knowledge-block">
                          <span className="editor-subtitle">
                            {activeKnowledgeDataset
                              ? t('ruleManage.dataCsv', { name: activeKnowledgeDataset })
                              : t('ruleManage.datasetCsv')}
                          </span>
                          <CodeEditor
                            key="knowledge-data"
                            className="code-area code-area--large"
                            value={knowledgeDatasetConfig.data || ''}
                            onChange={(value) =>
                              setKnowledgeDatasetConfig((prev) => ({ ...prev, data: value }))
                            }
                            language="plain"
                            theme="vscodeDark"
                          />
                        </div>
                      </>
                    )}
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
                    refreshWplFiles({
                        keyword: value,
                        page: 1,
                        preserveActive: true,
                      })
                        .catch((error) => {
                          message.error('加载 WPL 规则列表失败：' + error.message);
                        });
                    } else {
                      setOmlSearch(value);
                      refreshOmlFiles({
                        keyword: value,
                        page: 1,
                        preserveActive: true,
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
                                  {file.label}
                                </button>
                              ))}
                            </div>
                          ) : null}
                        </div>
                      );
                    })
                  : omlTree.map((node) => {
                      const expanded = omlExpandedGroups.includes(node.group);
                      return (
                        <div
                          key={node.group}
                          className="repo-file-group"
                          onMouseEnter={() => setHoveredRepoFile(node.group)}
                          onMouseLeave={() => setHoveredRepoFile('')}
                        >
                          <button
                            type="button"
                            className="repo-file repo-file--folder"
                            onClick={() => toggleOmlGroup(node.group)}
                            style={{
                              flex: 1,
                              display: 'flex',
                              alignItems: 'center',
                              justifyContent: 'space-between',
                            }}
                          >
                            <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                              <span aria-hidden="true">{expanded ? '📂' : '📁'}</span>
                              {node.group}
                            </span>
                            <span style={{ fontSize: 12, color: '#999' }}>{node.files.length}</span>
                          </button>
                          {expanded ? (
                            <div style={{ marginLeft: 24, marginTop: 4 }}>
                              {node.files.map((file) => (
                                <div
                                  key={file.value}
                                  style={{ position: 'relative' }}
                                  onMouseEnter={() => setHoveredRepoFile(file.value)}
                                  onMouseLeave={() => setHoveredRepoFile('')}
                                >
                                  <button
                                    type="button"
                                    className={`repo-file ${
                                      activeOmlFile === file.value ? 'is-active' : ''
                                    }`}
                                    onClick={() => {
                                      if (hasUnsavedChanges && file.value !== activeOmlFile) {
                                        Modal.confirm({
                                          title: t('ruleManage.leaveConfirm'),
                                          content: t('ruleManage.leaveConfirmMessage'),
                                          okText: t('common.confirm'),
                                          cancelText: t('common.cancel'),
                                          onOk: () => {
                                            setActiveOmlFile(file.value);
                                          },
                                        });
                                      } else {
                                        setActiveOmlFile(file.value);
                                      }
                                    }}
                                    style={{
                                      textAlign: 'left',
                                      paddingLeft: 32,
                                      paddingRight: hoveredRepoFile === file.value ? '28px' : '12px',
                                      position: 'relative',
                                    }}
                                  >
                                    {file.label}
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
                                      display: hoveredRepoFile === file.value ? 'inline-flex' : 'none',
                                      alignItems: 'center',
                                      justifyContent: 'center',
                                    }}
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      Modal.confirm({
                                        title: t('ruleManage.deleteConfirm'),
                                        content: t('ruleManage.deleteConfirmMessage', {
                                          filename: file.value,
                                        }),
                                        okText: t('common.delete'),
                                        okButtonProps: { danger: true },
                                        cancelText: t('common.cancel'),
                                        onOk: async () => {
                                          try {
                                            await deleteRuleFile({ type: RuleType.OML, file: file.value });
                                            const updated = omlFiles.filter(
                                              (name) => name !== file.value,
                                            );
                                            applyOmlListState(updated, {
                                              page: omlPage,
                                              preserveActive: false,
                                            });
                                          } catch (error) {
                                            message.error(
                                              t('ruleManage.deleteFailed', { message: error.message }),
                                            );
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
                          ) : null}
                        </div>
                      );
                    })}
              </div>
              {(activeKey === 'wpl'
                ? wplTotal > WPL_PAGE_SIZE
                : omlTotal > omlPageSize) ? (
                <div className="repo-pagination">
                  <Pagination
                    size="small"
                    simple
                    current={activeKey === 'wpl' ? wplPage : omlPage}
                    pageSize={activeKey === 'wpl' ? WPL_PAGE_SIZE : omlPageSize}
                    total={activeKey === 'wpl' ? wplTotal : omlTotal}
                    showSizeChanger={false}
                    onChange={(page) => {
                      if (activeKey === 'wpl') {
                        refreshWplFiles({
                          page,
                          preserveActive: true,
                        })
                          .catch((error) => {
                            message.error(t('ruleManage.loadWplFailed', { message: error.message }));
                          });
                        return;
                      }

                      refreshOmlFiles({
                        page,
                        preserveActive: true,
                      })
                        .catch((error) => {
                          message.error(t('ruleManage.loadOmlFailed', { message: error.message }));
                        });
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
                {displayedSinkFiles.map((item) => (
                  <button
                    key={item.file}
                    type="button"
                    className={`repo-file ${activeSinkFile === item.file ? 'is-active' : ''}`}
                    onClick={() => {
                      if (hasUnsavedChanges && item.file !== activeSinkFile) {
                        Modal.confirm({
                          title: t('ruleManage.leaveConfirm'),
                          content: t('ruleManage.leaveConfirmMessage'),
                          okText: t('common.confirm'),
                          cancelText: t('common.cancel'),
                          onOk: () => {
                            setActiveSinkFile(item.file);
                          },
                        });
                      } else {
                        setActiveSinkFile(item.file);
                      }
                    }}
                    style={{ textAlign: 'left' }}
                  >
                    {item.displayName}
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
