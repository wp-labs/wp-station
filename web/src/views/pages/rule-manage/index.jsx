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
import {
  omlCodeFormat,
  tomlCodeFormat,
  wfgCodeFormat,
  wflCodeFormat,
  wfsCodeFormat,
  wplCodeFormat,
} from '@/services/debug';
import { useSystem } from '@/contexts/SystemContext';
import CodeEditor from '@/views/components/CodeEditor/CodeEditor';
import ValidateResultModal from '@/components/ValidateResultModal';

const WPL_PAGE_SIZE = 14;
const OML_FOLDER_PAGE_SIZE = 50;
const OML_FETCH_PAGE_SIZE = 50;
const KNOWLEDGE_PAGE_SIZE = 15;
const EMPTY_KNOWLEDGE_DATASET = Object.freeze({
  createSql: '',
  insertSql: '',
  data: '',
});
const INTEGRATION_OVERVIEW_KEY = 'integration-overview';
const WFUSION_WINDOWS_FILE = 'windows.toml';
const WFUSION_GLOBAL_RULE_FILE = '_global.wfl';

const isWfusionGlobalRuleFile = (type, file) =>
  type === RuleType.RULE &&
  String(file || '')
    .trim()
    .split('/')
    .filter(Boolean)
    .pop()
    ?.toLowerCase() === WFUSION_GLOBAL_RULE_FILE;

const normalizeWplEntry = (value, parseFileName) => {
  if (value === undefined || value === null) {
    return '';
  }
  const trimmed = String(value).trim();
  if (!trimmed) {
    return '';
  }
  if (!trimmed.includes('/')) {
    return `${trimmed}/${parseFileName}`;
  }
  const [rulePart, ...restParts] = trimmed.split('/');
  const rule = (rulePart || '').trim();
  const sub = (restParts.join('/') || '').trim() || parseFileName;
  if (!rule) {
    return sub;
  }
  return `${rule}/${sub}`;
};

const normalizeWplList = (items, parseFileName) => {
  const deduped = new Set();
  (Array.isArray(items) ? items : []).forEach((item) => {
    const entry = normalizeWplEntry(item, parseFileName);
    if (entry) {
      deduped.add(entry);
    }
  });
  return Array.from(deduped);
};

const getWplEntryParts = (entry, parseFileName) => {
  if (!entry) {
    return { rule: '', sub: '' };
  }
  const normalized = normalizeWplEntry(entry, parseFileName);
  if (!normalized) {
    return { rule: '', sub: '' };
  }
  const [rule, sub] = normalized.split('/');
  return {
    rule: (rule || '').trim(),
    sub: (sub || '').trim(),
  };
};

const isWplSampleEntry = (entry, parseFileName, sampleFileName) =>
  normalizeWplEntry(entry, parseFileName).endsWith(`/${sampleFileName}`);
const isIgnoredWplIdentifier = (value) => String(value || '').trim().toLowerCase().startsWith('ignore');

const formatWplDisplayName = (entry, parseFileName) => {
  const { rule, sub } = getWplEntryParts(entry, parseFileName);
  if (!rule && !sub) {
    return '';
  }
  if (!rule) {
    return sub;
  }
  return `${rule}/${sub}`;
};

const formatOmlDisplayName = (entry) => {
  const normalized = String(entry || '').trim();
  if (!normalized) {
    return '';
  }

  const parts = normalized.split('/').filter(Boolean);
  const [group = '', ...rest] = parts;
  const groupDisplay = group
    ? group.toLowerCase().endsWith('.oml')
      ? group
      : `${group}.oml`
    : '';
  const fileDisplay = rest.length ? rest.join('/') : 'adm.oml';

  if (!groupDisplay) {
    return fileDisplay;
  }

  return `${groupDisplay}/${fileDisplay}`;
};

const formatNamedRuleDisplayName = (entry) => {
  const normalized = String(entry || '').trim();
  if (!normalized) {
    return '';
  }
  const parts = normalized.split('/').filter(Boolean);
  return parts[parts.length - 1] || normalized;
};

const normalizeNamedRuleEntry = (entry, type) => {
  const normalized = String(entry || '')
    .trim()
    .replace(/^\/+|\/+$/g, '');
  if (!normalized) {
    return '';
  }

  if (type !== RuleType.SCENARIOS) {
    return normalized;
  }

  const parts = normalized.split('/').filter(Boolean);
  const fileName = parts[parts.length - 1] || '';
  if (!fileName) {
    return '';
  }

  if (parts.length > 1) {
    return normalized.toLowerCase().endsWith('.wfg') ? normalized : `${normalized}.wfg`;
  }

  return normalized.toLowerCase().endsWith('.wfg') ? normalized : `${normalized}.wfg`;
};

const normalizeNamedRuleCreateFile = (repoType, name) => {
  const normalized = String(name || '')
    .trim()
    .replace(/^\/+|\/+$/g, '');
  if (!normalized) {
    return '';
  }

  const extension =
    repoType === 'schema' ? '.wfs' : repoType === 'rule' ? '.wfl' : '.wfg';
  const hasPath = normalized.includes('/');
  const hasExtension = normalized.toLowerCase().endsWith(extension);

  if (hasPath) {
    return hasExtension ? normalized : `${normalized}${extension}`;
  }

  if (hasExtension) {
    return normalized;
  }

  return `${normalized}/${normalized}${extension}`;
};

const buildWplTreeData = (items, parseFileName, sampleFileName) => {
  const groups = new Map();
  (Array.isArray(items) ? items : []).forEach((entry) => {
    const { rule } = getWplEntryParts(entry, parseFileName);
    if (!rule) {
      return;
    }
    groups.set(rule, [
      {
        value: `${rule}/${parseFileName}`,
        label: parseFileName,
        isSample: false,
      },
      {
        value: `${rule}/${sampleFileName}`,
        label: sampleFileName,
        isSample: true,
      },
    ]);
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

const parseTagAttributes = (rawTag = '') => {
  const attributes = {};
  const pattern = /([a-zA-Z0-9_]+)\s*:\s*"([^"]*)"/g;
  let match = pattern.exec(rawTag);
  while (match) {
    attributes[match[1]] = match[2];
    match = pattern.exec(rawTag);
  }
  return attributes;
};

const extractWplOverviewFromContent = (content = '', fallbackPackage = '') => {
  if (!content || typeof content !== 'string') {
    return null;
  }

  const packageMatch = content.match(
    /(?:#\[tag\(([\s\S]*?)\)\]\s*)?package\s+([a-zA-Z0-9_]+)\s*\{/m,
  );
  const packageTagAttributes = parseTagAttributes(packageMatch?.[1] || '');
  const packageKey = (packageMatch?.[2] || fallbackPackage || '').trim();
  if (!packageKey || isIgnoredWplIdentifier(packageKey)) {
    return null;
  }

  const deviceType =
    packageTagAttributes.dev_type?.trim() ||
    packageTagAttributes.dev_name?.trim() ||
    packageKey;

  const logTypes = [];
  const seenRules = new Set();
  const rulePattern = /(?:#\[tag\(([\s\S]*?)\)\]\s*)?rule\s+([a-zA-Z0-9_]+)\s*\{/g;
  let ruleMatch = rulePattern.exec(content);
  while (ruleMatch) {
    const ruleKey = (ruleMatch[2] || '').trim();
    if (ruleKey && !seenRules.has(ruleKey) && !isIgnoredWplIdentifier(ruleKey)) {
      seenRules.add(ruleKey);
      const ruleTagAttributes = parseTagAttributes(ruleMatch[1] || '');
      logTypes.push({
        ruleKey,
        logTypeName: ruleTagAttributes.log_desc?.trim() || ruleKey,
      });
    }
    ruleMatch = rulePattern.exec(content);
  }

  return {
    packageKey,
    deviceType,
    logTypes,
  };
};

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

const buildNamedRuleTreeData = (items) => {
  const groups = new Map();
  const flatFiles = [];

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

    if (parts.length === 1) {
      flatFiles.push({
        kind: 'file',
        key: normalized,
        file: {
          value: normalized,
          label: normalized,
        },
      });
      return;
    }

    const [group, ...rest] = parts;
    if (!group) {
      return;
    }

    const files = groups.get(group) || [];
    files.push({
      value: normalized,
      label: rest.join('/'),
    });
    groups.set(group, files);
  });

  const groupNodes = Array.from(groups.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([group, files]) => ({
      kind: 'group',
      key: group,
      group,
      files: files.sort((a, b) => a.label.localeCompare(b.label)),
    }));

  return [...flatFiles, ...groupNodes].sort((a, b) => {
    const aLabel = a.kind === 'group' ? a.group : a.file.label;
    const bLabel = b.kind === 'group' ? b.group : b.file.label;
    if (aLabel === WFUSION_GLOBAL_RULE_FILE) {
      return -1;
    }
    if (bLabel === WFUSION_GLOBAL_RULE_FILE) {
      return 1;
    }
    return aLabel.localeCompare(bLabel);
  });
};

const normalizeOmlList = (items, type) => {
  const deduped = new Set();
  (Array.isArray(items) ? items : []).forEach((item) => {
    if (typeof item !== 'string') {
      return;
    }
    const normalized = normalizeNamedRuleEntry(item, type);
    if (normalized) {
      deduped.add(normalized);
    }
  });
  return Array.from(deduped);
};

const getOmlEntriesFromTreeData = (treeData) =>
  (Array.isArray(treeData) ? treeData : []).flatMap((node) =>
    node.kind === 'file'
      ? [node.file?.value].filter(Boolean)
      : (Array.isArray(node.files) ? node.files : [])
          .map((item) => item.value)
          .filter(Boolean),
  );

const getFirstOmlEntry = (treeData) => treeData?.[0]?.files?.[0]?.value || '';
const getFirstNamedRuleEntry = (treeData) => {
  for (const node of treeData || []) {
    if (node?.kind === 'file' && node.file?.value) {
      return node.file.value;
    }
    if (Array.isArray(node?.files) && node.files[0]?.value) {
      return node.files[0].value;
    }
  }
  return '';
};

const findOmlGroupForFile = (treeData, file) => {
  if (!file) {
    return '';
  }
  for (const node of treeData || []) {
    if (node?.kind === 'file' && node.file?.value === file) {
      return '';
    }
    if (Array.isArray(node.files) && node.files.some((item) => item.value === file)) {
      return node.group;
    }
  }
  return '';
};

const isTreeRuleType = (type) =>
  type === RuleType.OML ||
  type === RuleType.SCHEMA ||
  type === RuleType.RULE ||
  type === RuleType.SCENARIOS;

const getDefaultRuleManageKey = (system) =>
  system === 'wfusion' ? RuleType.WINDOWS : RuleType.WPL;

const getTreeRuleLoadErrorKey = (type) => {
  if (type === RuleType.SCHEMA) {
    return 'ruleManage.loadSchemaFailed';
  }
  if (type === RuleType.RULE) {
    return 'ruleManage.loadRuleFailed';
  }
  if (type === RuleType.SCENARIOS) {
    return 'ruleManage.loadScenariosFailed';
  }
  return 'ruleManage.loadOmlFailed';
};

/**
 * 规则配置管理页面
 * 功能：
 * 1. 显示和编辑各类规则配置（wpl/oml/knowledge）
 * 2. 支持配置校验和保存
 * 对应原型：pages/views/rule-manage/source-config.html
 */
function RuleManagePage() {
  const { t } = useTranslation();
  const { currentSystem, registerBeforeSystemSwitch } = useSystem();
  const isWfusionSystem = currentSystem === 'wfusion';
  
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
      schema: {
        title: t('ruleManage.addSchemaRule'),
        placeholder: t('ruleManage.schemaRuleNamePlaceholder'),
        tip: t('ruleManage.schemaRuleTip'),
      },
      rule: {
        title: t('ruleManage.addWfusionRule'),
        placeholder: t('ruleManage.wfusionRuleNamePlaceholder'),
        tip: t('ruleManage.wfusionRuleTip'),
      },
      scenarios: {
        title: t('ruleManage.addScenarioRule'),
        placeholder: t('ruleManage.scenarioRuleNamePlaceholder'),
        tip: t('ruleManage.scenarioRuleTip'),
      },
      knowledge: {
        title: t('ruleManage.addDataset'),
        placeholder: t('ruleManage.datasetNamePlaceholder'),
        tip: t('ruleManage.datasetTip'),
      },
    };
    return meta[type] || {};
  };
  
  const [activeKey, setActiveKey] = useState(() => getDefaultRuleManageKey(currentSystem));
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  
  // wpl 配置的子文件列表
  const [allWplFiles, setAllWplFiles] = useState([]);
  const [activeWplFile, setActiveWplFile] = useState('');
  const [localWplFiles, setLocalWplFiles] = useState([]);
  const [wplPage, setWplPage] = useState(1);
  const [wplTree, setWplTree] = useState([]);
  const [wplExpandedRules, setWplExpandedRules] = useState([]);
  const [wplOverviewItems, setWplOverviewItems] = useState([]);
  const [wplOverviewExpandedDevices, setWplOverviewExpandedDevices] = useState([]);
  const [wplOverviewLoading, setWplOverviewLoading] = useState(false);
  const [activeOverviewRuleKey, setActiveOverviewRuleKey] = useState('');
  
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
  const [validateModalVisible, setValidateModalVisible] = useState(false);
  const [validateResult, setValidateResult] = useState(null);
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
  // 跟踪 wpl/oml 列表当前悬停的文件名，用于展示删除按钮
  const [hoveredRepoFile, setHoveredRepoFile] = useState('');
  const [wplSearch, setWplSearch] = useState('');
  const [omlSearch, setOmlSearch] = useState('');
  const [knowledgeSearch, setKnowledgeSearch] = useState('');
  const [ruleFilesMeta, setRuleFilesMeta] = useState({
    wplParseFile: '',
    wplSampleFile: '',
    knowledgeConfigFile: '',
  });
  const ruleFilesMetaRef = React.useRef({
    wplParseFile: '',
    wplSampleFile: '',
    knowledgeConfigFile: '',
  });
  const activeWplFileRef = React.useRef('');
  const activeOmlFileRef = React.useRef('');
  const wplOverviewCacheRef = React.useRef(new Map());
  const wplParseFile = ruleFilesMeta.wplParseFile;
  const wplSampleFile = ruleFilesMeta.wplSampleFile;
  const knowledgeConfigFile = ruleFilesMeta.knowledgeConfigFile;
  const isWplEditorMode =
    !isWfusionSystem && (activeKey === RuleType.WPL || activeKey === INTEGRATION_OVERVIEW_KEY);
  const isIntegrationOverviewMode = activeKey === INTEGRATION_OVERVIEW_KEY;
  const isKnowledgeMode = activeKey === RuleType.KNOWLEDGE;
  const isKnowdbEditorMode =
    isKnowledgeMode &&
    Boolean(knowledgeConfigFile) &&
    activeKnowledgeDataset === knowledgeConfigFile;
  const isFixedRuleMode = activeKey === RuleType.WINDOWS;
  const activeTreeRuleType = isTreeRuleType(activeKey) ? activeKey : null;
  const isRepoMode = isWplEditorMode || Boolean(activeTreeRuleType);
  const treeSectionTitle =
    activeTreeRuleType === RuleType.SCHEMA
      ? t('ruleManage.schemaRules')
      : activeTreeRuleType === RuleType.RULE
        ? t('ruleManage.wfusionRules')
        : activeTreeRuleType === RuleType.SCENARIOS
          ? t('ruleManage.scenariosRules')
        : t('ruleManage.enrichmentRules');
  const treeSearchPlaceholder =
    activeTreeRuleType === RuleType.SCHEMA
      ? t('ruleManage.searchSchemaRules')
      : activeTreeRuleType === RuleType.RULE
        ? t('ruleManage.searchWfusionRules')
        : activeTreeRuleType === RuleType.SCENARIOS
          ? t('ruleManage.searchScenariosRules')
        : t('ruleManage.searchEnrichmentRules');
  const treeAriaLabel =
    activeTreeRuleType === RuleType.SCHEMA
      ? t('ruleManage.schemaRules')
      : activeTreeRuleType === RuleType.RULE
        ? t('ruleManage.wfusionRules')
        : activeTreeRuleType === RuleType.SCENARIOS
          ? t('ruleManage.scenariosRules')
        : t('ruleManage.enrichmentRules');
  const currentFormatTarget =
    isWplEditorMode && isWplSampleEntry(activeWplFile, wplParseFile, wplSampleFile)
      ? null
      : isWplEditorMode
        ? 'wpl'
        : isKnowdbEditorMode
          ? 'toml'
          : isFixedRuleMode
            ? 'toml'
            : activeTreeRuleType === RuleType.OML
              ? 'oml'
              : activeTreeRuleType === RuleType.SCHEMA
                ? 'wfs'
                : activeTreeRuleType === RuleType.RULE
                  ? 'wfl'
                  : activeTreeRuleType === RuleType.SCENARIOS
                    ? 'wfg'
                    : null;
  const showFormatButton = Boolean(currentFormatTarget);

  useEffect(() => {
    const allowedKeys = isWfusionSystem
      ? [RuleType.WINDOWS, RuleType.SCHEMA, RuleType.RULE, RuleType.SCENARIOS]
      : [RuleType.WPL, RuleType.OML, RuleType.KNOWLEDGE, INTEGRATION_OVERVIEW_KEY];

    if (!allowedKeys.includes(activeKey)) {
      setActiveKey(getDefaultRuleManageKey(currentSystem));
    }
  }, [activeKey, currentSystem, isWfusionSystem]);

  useEffect(() => {
    activeWplFileRef.current = activeWplFile;
  }, [activeWplFile]);

  useEffect(() => {
    activeOmlFileRef.current = activeOmlFile;
  }, [activeOmlFile]);

  const confirmBeforeLeaveCurrentEditor = useCallback(() => {
    if (!hasUnsavedChanges) {
      return Promise.resolve(true);
    }

    return new Promise((resolve) => {
      Modal.confirm({
        title: t('ruleManage.leaveConfirm'),
        content: t('ruleManage.leaveConfirmMessage'),
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

  useEffect(() => registerBeforeSystemSwitch(() => confirmBeforeLeaveCurrentEditor()), [
    confirmBeforeLeaveCurrentEditor,
    registerBeforeSystemSwitch,
  ]);

  const applyRuleFilesMeta = useCallback((meta) => {
    setRuleFilesMeta((prev) => {
      const next = {
        wplParseFile: meta?.wplParseFile || prev.wplParseFile,
        wplSampleFile: meta?.wplSampleFile || prev.wplSampleFile,
        knowledgeConfigFile: meta?.knowledgeConfigFile || prev.knowledgeConfigFile,
      };
      ruleFilesMetaRef.current = next;
      return next;
    });
  }, []);
  
  const totalKnowledgePages = Math.max(
    1,
    Math.ceil(Math.max(knowledgeTotal, 1) / KNOWLEDGE_PAGE_SIZE),
  );
  const pagedKnowledgeDatasets = knowledgeDatasets; // 当前页数据由后端提供
  const knowledgeListForDisplay = React.useMemo(() => {
    const datasets = Array.isArray(pagedKnowledgeDatasets) ? pagedKnowledgeDatasets : [];
    return [ruleFilesMeta.knowledgeConfigFile, ...datasets].filter(Boolean);
  }, [pagedKnowledgeDatasets, ruleFilesMeta.knowledgeConfigFile]);
  
  /**
   * 加载配置内容
   */
  const loadConfig = async () => {
    // 调用服务层获取配置（使用对象参数）
    const options = { type: activeKey };
    if (isWplEditorMode) {
      const targetFile = normalizeWplEntry(activeWplFile, ruleFilesMeta.wplParseFile);
      if (!targetFile) {
        setContent('');
        return;
      }
      const targetRule = getWplEntryParts(targetFile, ruleFilesMeta.wplParseFile).rule;
      if (targetRule && localWplFiles.includes(targetRule)) {
        setContent('');
        return;
      }
      options.type = RuleType.WPL;
      options.file = targetFile;
    } else if (isFixedRuleMode) {
      options.file = WFUSION_WINDOWS_FILE;
    } else if (activeTreeRuleType) {
      if (!activeOmlFile) {
        setContent('');
        return;
      }
      const normalizedActiveFile = normalizeNamedRuleEntry(activeOmlFile, activeTreeRuleType);
      if (localOmlFiles.includes(normalizedActiveFile || activeOmlFile)) {
        setContent('');
        return;
      }
      options.file = normalizedActiveFile || activeOmlFile;
    } else if (isKnowledgeMode) {
      if (!activeKnowledgeDataset) {
        setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
        return;
      }
      if (activeKnowledgeDataset === ruleFilesMeta.knowledgeConfigFile) {
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
    }
    
    setLoading(true);
    try {
      const response = await fetchRuleConfig(options);
      if (isKnowledgeMode) {
        if (activeKnowledgeDataset === ruleFilesMeta.knowledgeConfigFile) {
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
      message.error(t('ruleManage.loadFailed', { message: error?.message || error }));
      // 加载失败时设置为空
      if (isKnowledgeMode) {
        if (activeKnowledgeDataset === knowledgeConfigFile) {
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
      const { preferredActive, preserveActive, page, total, meta } = options;
      const parseFileName = meta?.wplParseFile || ruleFilesMetaRef.current.wplParseFile;
      const sampleFileName = meta?.wplSampleFile || ruleFilesMetaRef.current.wplSampleFile;
      const normalizedList = normalizeWplList(rawItems, parseFileName);
      setAllWplFiles(normalizedList);
      const treeData = buildWplTreeData(normalizedList, parseFileName, sampleFileName);
      setWplTotal(typeof total === 'number' && total >= 0 ? total : treeData.length);

      if (!normalizedList.length) {
        setWplTree([]);
        setWplPage(typeof page === 'number' && page > 0 ? page : 1);
        setWplExpandedRules([]);
        setActiveWplFile('');
        return;
      }

      const normalizedPreferred = preferredActive
        ? normalizeWplEntry(preferredActive, parseFileName)
        : '';
      const normalizedActive = normalizeWplEntry(activeWplFileRef.current, parseFileName);
      const currentPage = typeof page === 'number' && page > 0 ? page : 1;
      const currentTreeData = treeData;
      const currentPageFiles = currentTreeData.flatMap((node) =>
        (Array.isArray(node.files) ? node.files : []).map((item) => item.value).filter(Boolean),
      );

      let nextActive = currentPageFiles.includes(normalizedPreferred) ? normalizedPreferred : null;
      if (!nextActive && preserveActive && currentPageFiles.includes(activeWplFileRef.current)) {
        nextActive = activeWplFileRef.current;
      }
      if (!nextActive && preserveActive && currentPageFiles.includes(normalizedActive)) {
        nextActive = normalizedActive;
      }

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
    [],
  );

  const applyOmlListState = useCallback(
    (rawItems, options = {}) => {
      const { preferredActive, preserveActive, page, total, type } = options;
      const normalizedList = normalizeOmlList(rawItems, type);
      const isNamedRuleType =
        type === RuleType.SCHEMA ||
        type === RuleType.RULE ||
        type === RuleType.SCENARIOS;
      const treeData = isNamedRuleType
        ? buildNamedRuleTreeData(normalizedList)
        : buildOmlTreeData(normalizedList);

      setOmlFiles(normalizedList);
      setOmlTotal(typeof total === 'number' && total >= 0 ? total : treeData.length);

      if (!normalizedList.length) {
        setOmlTree([]);
        setOmlPage(typeof page === 'number' && page > 0 ? page : 1);
        setOmlExpandedGroups([]);
        setActiveOmlFile('');
        return;
      }

      const normalizedPreferredValue = normalizeNamedRuleEntry(preferredActive, type);
      const normalizedActiveValue = normalizeNamedRuleEntry(activeOmlFileRef.current, type);
      const normalizedPreferred =
        normalizedPreferredValue && normalizedList.includes(normalizedPreferredValue)
          ? normalizedPreferredValue
          : '';
      const normalizedActive = normalizedList.includes(normalizedActiveValue)
        ? normalizedActiveValue
        : '';

      const currentPage = typeof page === 'number' && page > 0 ? page : 1;
      const currentTreeData = treeData;
      const currentPageFiles = isNamedRuleType
        ? getOmlEntriesFromTreeData(currentTreeData)
        : getOmlEntriesFromTreeData(currentTreeData);

      let nextActive = currentPageFiles.includes(normalizedPreferred) ? normalizedPreferred : '';
      if (!nextActive && preserveActive && currentPageFiles.includes(normalizedActive)) {
        nextActive = normalizedActive;
      }
      if (!nextActive) {
        nextActive = (isNamedRuleType
          ? getFirstNamedRuleEntry(currentTreeData)
          : getFirstOmlEntry(currentTreeData)) || currentPageFiles[0] || '';
      }

      setOmlTree(currentTreeData);
      setOmlPage(currentPage);
      setActiveOmlFile(nextActive);
      const expandedGroups = currentTreeData
        .filter((node) => node.kind !== 'file')
        .map((node) => node.group);
      const activeGroup = findOmlGroupForFile(currentTreeData, nextActive);
      setOmlExpandedGroups(
        activeGroup && !expandedGroups.includes(activeGroup)
          ? [...expandedGroups, activeGroup]
          : expandedGroups,
      );
    },
    [omlPageSize],
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
      applyRuleFilesMeta(result?.meta);
      const rawItems = Array.isArray(result?.items) ? result.items : [];
      const items =
        type === RuleType.WPL
          ? normalizeWplList(
              rawItems,
              result?.meta?.wplParseFile || ruleFilesMetaRef.current.wplParseFile,
            )
          : normalizeOmlList(rawItems, type);
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
  }, [applyRuleFilesMeta]);

  const refreshWplFiles = useCallback(
    async (options = {}) => {
      const {
        keyword = wplSearch,
        page,
        preferredActive,
        preserveActive = true,
      } = options;
      const result = await fetchRuleFiles({
        type: RuleType.WPL,
        page: typeof page === 'number' && page > 0 ? page : 1,
        pageSize: WPL_PAGE_SIZE,
        keyword: keyword?.trim() ? keyword.trim() : undefined,
      });
      applyRuleFilesMeta(result?.meta);
      const files = Array.isArray(result?.items) ? result.items : [];
      applyWplListState(files, {
        meta: result?.meta,
        page: result?.page || page || 1,
        total: typeof result?.total === 'number' ? result.total : files.length,
        preferredActive,
        preserveActive,
      });
      return files;
    },
    [applyRuleFilesMeta, applyWplListState, wplSearch],
  );

  const refreshOmlFiles = useCallback(
    async (options = {}) => {
      const {
        type = activeTreeRuleType || RuleType.OML,
        keyword = omlSearch,
        page,
        preferredActive,
        preserveActive = true,
      } = options;
      const result = await fetchRuleFiles({
        type,
        page: typeof page === 'number' && page > 0 ? page : 1,
        pageSize: omlPageSize,
        keyword: keyword?.trim() ? keyword.trim() : undefined,
      });
      const files = Array.isArray(result?.items) ? result.items : [];
      applyOmlListState(files, {
        type,
        page: result?.page || page || 1,
        total: typeof result?.total === 'number' ? result.total : files.length,
        preferredActive,
        preserveActive,
      });
      return files;
    },
    [activeTreeRuleType, applyOmlListState, omlPageSize, omlSearch],
  );

  useEffect(() => {
    if (isWfusionSystem) {
      return;
    }
    refreshWplFiles({ preserveActive: true, page: 1 }).catch((error) => {
      message.error(t('ruleManage.loadWplFailed', { message: error.message }));
    });
  }, [isWfusionSystem, refreshWplFiles, t]);

  useEffect(() => {
    if (!activeTreeRuleType) {
      return;
    }

    refreshOmlFiles({ type: activeTreeRuleType, preserveActive: true, page: 1 }).catch((error) => {
      message.error(t(getTreeRuleLoadErrorKey(activeTreeRuleType), { message: error.message }));
    });
  }, [activeTreeRuleType, refreshOmlFiles, t]);

  useEffect(() => {
    if (!isIntegrationOverviewMode) {
      return undefined;
    }

    let cancelled = false;

    const loadWplOverview = async () => {
      const packageKeys = Array.from(
        new Set(
          (Array.isArray(allWplFiles) ? allWplFiles : [])
            .filter((entry) =>
              !isWplSampleEntry(
                entry,
                ruleFilesMeta.wplParseFile,
                ruleFilesMeta.wplSampleFile,
              ),
            )
            .map((entry) => getWplEntryParts(entry, ruleFilesMeta.wplParseFile).rule)
            .filter(Boolean),
        ),
      );

      if (!packageKeys.length) {
        setWplOverviewItems([]);
        setWplOverviewExpandedDevices([]);
        return;
      }

      setWplOverviewLoading(true);
      try {
        const overviewResults = await Promise.all(
          packageKeys.map(async (packageKey) => {
            const file = `${packageKey}/${ruleFilesMeta.wplParseFile}`;
            const cached = wplOverviewCacheRef.current.get(file);
            if (cached) {
              return cached;
            }
            const response = await fetchRuleConfig({ type: RuleType.WPL, file });
            const parsed = extractWplOverviewFromContent(response?.content || '', packageKey);
            if (parsed) {
              wplOverviewCacheRef.current.set(file, parsed);
            }
            return parsed;
          }),
        );

        if (cancelled) {
          return;
        }

        const nextItems = overviewResults
          .filter(Boolean)
          .sort((a, b) => a.deviceType.localeCompare(b.deviceType, 'zh-Hans-CN'));

        setWplOverviewItems(nextItems);
        setWplOverviewExpandedDevices((prev) => {
          if (!prev.length) {
            return nextItems.slice(0, 6).map((item) => item.packageKey);
          }
          return prev.filter((key) => nextItems.some((item) => item.packageKey === key));
        });
      } catch (error) {
        if (!cancelled) {
          message.error(t('ruleManage.loadIntegrationOverviewFailed', { message: error.message }));
        }
      } finally {
        if (!cancelled) {
          setWplOverviewLoading(false);
        }
      }
    };

    loadWplOverview();

    return () => {
      cancelled = true;
    };
  }, [
    allWplFiles,
    isIntegrationOverviewMode,
    ruleFilesMeta.wplParseFile,
    ruleFilesMeta.wplSampleFile,
    t,
  ]);

  const handleSelectWplFile = (fileValue, options = {}) => {
    const { ruleKey = '' } = options;
    if (!fileValue) {
      return;
    }

    const applySelection = () => {
      setActiveWplFile(fileValue);
      setActiveOverviewRuleKey(ruleKey);
    };

    if (hasUnsavedChanges && fileValue !== activeWplFile) {
      Modal.confirm({
        title: t('ruleManage.leaveConfirm'),
        content: t('ruleManage.leaveConfirmMessage'),
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: applySelection,
      });
      return;
    }

    applySelection();
  };

  const toggleWplOverviewDevice = (packageKey) => {
    setWplOverviewExpandedDevices((prev) =>
      prev.includes(packageKey) ? prev.filter((item) => item !== packageKey) : [...prev, packageKey],
    );
  };

  const wplOverviewStats = React.useMemo(
    () => ({
      deviceTypeCount: wplOverviewItems.length,
      logTypeCount: wplOverviewItems.reduce(
        (total, item) => total + (Array.isArray(item.logTypes) ? item.logTypes.length : 0),
        0,
      ),
    }),
    [wplOverviewItems],
  );

  const updateWplOverviewCacheForFile = (file, nextContent) => {
    const normalizedFile = normalizeWplEntry(file, wplParseFile);
    const { rule } = getWplEntryParts(normalizedFile, wplParseFile);
    if (!normalizedFile || !rule || !normalizedFile.endsWith(`/${wplParseFile}`)) {
      return;
    }

    const parsed = extractWplOverviewFromContent(nextContent, rule);
    if (parsed) {
      wplOverviewCacheRef.current.set(normalizedFile, parsed);
    } else {
      wplOverviewCacheRef.current.delete(normalizedFile);
    }

    setWplOverviewItems((prev) => {
      const nextItems = prev.filter((item) => item.packageKey !== rule);
      if (parsed) {
        nextItems.push(parsed);
      }
      return nextItems.sort((a, b) => a.deviceType.localeCompare(b.deviceType, 'zh-Hans-CN'));
    });
  };

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

  const handleSelectTreeFile = (fileValue) => {
    if (!fileValue) {
      return;
    }

    const applySelection = () => {
      setActiveOmlFile(fileValue);
    };

    if (hasUnsavedChanges && fileValue !== activeOmlFile) {
      Modal.confirm({
        title: t('ruleManage.leaveConfirm'),
        content: t('ruleManage.leaveConfirmMessage'),
        okText: t('common.confirm'),
        cancelText: t('common.cancel'),
        onOk: applySelection,
      });
      return;
    }

    applySelection();
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

  const confirmDeleteTreeRule = (ruleType, fileName) => {
    Modal.confirm({
      title: t('ruleManage.deleteConfirm'),
      content: t('ruleManage.deleteConfirmMessage', {
        filename: ruleType === RuleType.OML ? formatOmlDisplayName(fileName) : fileName,
      }),
      okText: t('common.delete'),
      okButtonProps: { danger: true },
      cancelText: t('common.cancel'),
      onOk: async () => {
        try {
          await deleteRuleFile({ type: ruleType, file: fileName });
          const updated = omlFiles.filter((name) => name !== fileName);
          applyOmlListState(updated, {
            type: ruleType,
            page: omlPage,
            preserveActive: false,
          });
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
    if (isTreeRuleType(repoType)) {
      await refreshOmlFiles({ type: repoType, preserveActive: true, page: omlPage });
    }
  };

  const resetTreeRuleSelection = useCallback(() => {
    setActiveOmlFile('');
    setOmlFiles([]);
    setOmlTree([]);
    setOmlExpandedGroups([]);
    setContent('');
    setOriginalContent('');
  }, []);

  const prepareTreeRuleNavigation = useCallback(
    (targetType) => {
      if (activeKey !== targetType) {
        resetTreeRuleSelection();
      }
      setLocalOmlFiles([]);
    },
    [activeKey, resetTreeRuleSelection],
  );

  // 当配置类型或子文件变化时重新加载
  useEffect(() => {
    loadConfig();
  }, [
    activeKey,
    activeWplFile,
    activeOmlFile,
    activeKnowledgeDataset,
    // 注意：localWplFiles/localOmlFiles 不放入依赖，避免一次加载导致重复请求
  ]);

  useEffect(() => {
    if (!activeWplFile || !activeOverviewRuleKey) {
      return;
    }
    const { rule } = getWplEntryParts(activeWplFile, wplParseFile);
    const stillMatched = wplOverviewItems.some(
      (item) =>
        item.packageKey === rule &&
        item.logTypes.some((logType) => logType.ruleKey === activeOverviewRuleKey),
    );
    if (!stillMatched) {
      setActiveOverviewRuleKey('');
    }
  }, [activeOverviewRuleKey, activeWplFile, wplOverviewItems]);

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
    if (isKnowledgeMode) {
      if (activeKnowledgeDataset === knowledgeConfigFile) {
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
    if (isKnowledgeMode && activeKnowledgeDataset === knowledgeConfigFile) {
      message.info('knowdb.toml 不需要执行独立校验');
      return;
    }
    const currentContent = buildCurrentContent();
    try {
      // 调用服务层校验配置
      const response = await validateRuleConfig({
        type: isWplEditorMode ? RuleType.WPL : activeKey,
        file: fileInfo.file,
        content: currentContent,
      });

      setValidateResult({
        filename: response.filename || fileInfo.display || fileInfo.file,
        valid: Boolean(response.valid),
        message: response.message || null,
        details: response.details || [],
        type: typeLabelMap[isWplEditorMode ? RuleType.WPL : activeKey] || activeKey,
      });
      setValidateModalVisible(true);
    } catch (error) {
      setValidateResult({
        filename: fileInfo.display || fileInfo.file,
        valid: false,
        message: error.message || '未知错误',
        details: [],
        type: typeLabelMap[isWplEditorMode ? RuleType.WPL : activeKey] || activeKey,
      });
      setValidateModalVisible(true);
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

    try {
      if (isKnowledgeMode) {
        if (activeKnowledgeDataset === knowledgeConfigFile) {
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
        const currentContent = buildCurrentContent();
        // 直接调用服务层保存配置（使用对象参数），不再弹出确认框
        await saveRuleConfig({
          type: isWplEditorMode ? RuleType.WPL : activeKey,
          file: fileInfo.file,
          content: currentContent,
        });
        if (isWplEditorMode) {
          updateWplOverviewCacheForFile(fileInfo.file, currentContent);
        }
      }

      // 保存成功后重置未保存状态
      if (!isKnowledgeMode) {
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
    if (!currentFormatTarget) {
      return;
    }

    const sourceContent = isKnowdbEditorMode ? knowdbConfig : content;
    if (!sourceContent || sourceContent.trim() === '') {
      message.warning(t('common.noFormatContent'));
      return;
    }

    try {
      let result;
      switch (currentFormatTarget) {
        case 'wpl':
          result = await wplCodeFormat(sourceContent);
          break;
        case 'oml':
          result = await omlCodeFormat(sourceContent);
          break;
        case 'wfs':
          result = await wfsCodeFormat(sourceContent);
          break;
        case 'wfl':
          result = await wflCodeFormat(sourceContent);
          break;
        case 'wfg':
          result = await wfgCodeFormat(sourceContent);
          break;
        case 'toml':
          result = await tomlCodeFormat(sourceContent);
          break;
        default:
          return;
      }

      const formattedCode =
        result.wpl_code ||
        result.oml_code ||
        result.wfs_code ||
        result.wfl_code ||
        result.wfg_code ||
        result.toml_code ||
        '';

      if (formattedCode && formattedCode !== sourceContent) {
        if (isKnowdbEditorMode) {
          setKnowdbConfig(formattedCode);
        } else {
          setContent(formattedCode);
        }
        message.success(t('ruleManage.format'));
      } else {
        message.info(t('ruleManage.format'));
      }
    } catch (error) {
      const errorKey =
        currentFormatTarget === 'oml'
          ? 'simulateDebug.omlInput.formatError'
          : currentFormatTarget === 'toml'
            ? 'debug.toml.formatError'
            : currentFormatTarget === 'wfs'
              ? 'debug.wfs.formatError'
              : currentFormatTarget === 'wfl'
                ? 'debug.wfl.formatError'
                : currentFormatTarget === 'wfg'
                  ? 'debug.wfg.formatError'
                  : 'simulateDebug.parseRule.formatError';
      message.error(error?.message || t(errorKey));
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
      await createRuleFile({ type: RuleType.WPL, file: normalizedName });
      await saveRuleConfig({ type: RuleType.WPL, file: normalizedName, content: '' });
      await refreshWplFiles({
        page: 1,
        preferredActive: `${normalizedName}/${wplParseFile}`,
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
        type: RuleType.OML,
        preferredActive: normalizedName,
        preserveActive: false,
      });
      setLocalOmlFiles((prev) => prev.filter((name) => name !== normalizedName));
      setContent('');
      return true;
    }

    if (repoType === 'schema' || repoType === 'rule' || repoType === 'scenarios') {
      const virtualFile = normalizeNamedRuleCreateFile(repoType, normalizedName);
      if (!virtualFile) {
        message.warning(t('ruleManage.fileNameCannotBeEmpty'));
        return false;
      }
      const targetType =
        repoType === 'schema'
          ? RuleType.SCHEMA
          : repoType === 'rule'
            ? RuleType.RULE
            : RuleType.SCENARIOS;
      await createRuleFile({
        type: targetType,
        file: virtualFile,
      });
      await saveRuleConfig({
        type: targetType,
        file: virtualFile,
        content: '',
      });
      await refreshOmlFiles({
        type: targetType,
        preferredActive: virtualFile,
        preserveActive: false,
      });
      setLocalOmlFiles((prev) => prev.filter((name) => name !== virtualFile));
      setContent('');
      return true;
    }

    return false;
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
        applyRuleFilesMeta(refreshed?.meta);
        const datasets = Array.isArray(refreshed?.items) ? refreshed.items : [];
        const normalizedDatasets = datasets.filter((item) => item !== knowledgeConfigFile);
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

  const typeLabelMap = {
    wpl: 'WPL',
    oml: 'OML',
    windows: 'Windows',
    schema: 'Schema',
    rule: 'Rule',
    scenarios: 'Scenarios',
    knowledge: 'Knowledge',
  };

  // 获取页面标题（与旧版本一致）
  const getPageTitle = () => {
    const titles = {
      wpl: t('ruleManage.wplConfig'),
      oml: t('ruleManage.omlConfig'),
      windows: t('ruleManage.windowsConfig'),
      schema: t('ruleManage.schemasConfig'),
      rule: t('ruleManage.rulesConfig'),
      scenarios: t('ruleManage.scenariosConfig'),
      knowledge: t('ruleManage.knowledgeConfig'),
      [INTEGRATION_OVERVIEW_KEY]: t('ruleManage.integrationOverview'),
    };
    return titles[activeKey] || t('ruleManage.title');
  };

  const getCurrentFileInfo = () => {
    if (isWplEditorMode) {
      const normalized = normalizeWplEntry(activeWplFile, wplParseFile);
      return { file: normalized, display: formatWplDisplayName(normalized, wplParseFile) };
    }
    if (isFixedRuleMode) {
      return { file: WFUSION_WINDOWS_FILE, display: WFUSION_WINDOWS_FILE };
    }
    if (activeTreeRuleType) {
      const displayName =
        activeTreeRuleType === RuleType.OML
          ? formatOmlDisplayName(activeOmlFile)
          : formatNamedRuleDisplayName(activeOmlFile);
      return { file: activeOmlFile || '', display: displayName };
    }
    if (isKnowledgeMode) {
      if (activeKnowledgeDataset === knowledgeConfigFile) {
        return { file: knowledgeConfigFile, display: knowledgeConfigFile };
      }
      const datasetName = activeKnowledgeDataset ? `${activeKnowledgeDataset}.dataset` : '';
      return { file: datasetName, display: datasetName };
    }
    return { file: '', display: '' };
  };

  const buildCurrentContent = () => {
    if (isKnowledgeMode) {
      if (activeKnowledgeDataset === knowledgeConfigFile) {
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

  const codeEditorLanguage =
    isWplEditorMode
      ? isWplSampleEntry(activeWplFile, wplParseFile, wplSampleFile)
        ? 'plain'
        : 'wpl'
      : isFixedRuleMode
        ? 'toml'
      : activeTreeRuleType === RuleType.OML
        ? 'oml'
        : activeTreeRuleType === RuleType.SCHEMA
          ? 'wfs'
          : activeTreeRuleType === RuleType.RULE
            ? 'wfl'
            : activeTreeRuleType === RuleType.SCENARIOS
              ? 'wfg'
              : 'plain';

  return (
    <>
      {/* 左侧侧边栏 */}
      <aside className="side-nav" data-group="rule-manage">
        <h2>{t('ruleManage.title')}</h2>
        {isWfusionSystem ? (
          <>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.WINDOWS ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.WINDOWS)}
            >
              {t('ruleManage.windowsConfig')}
            </button>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.SCHEMA ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.SCHEMA, () => {
                prepareTreeRuleNavigation(RuleType.SCHEMA);
              })}
            >
              {t('ruleManage.schemasConfig')}
            </button>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.RULE ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.RULE, () => {
                prepareTreeRuleNavigation(RuleType.RULE);
              })}
            >
              {t('ruleManage.rulesConfig')}
            </button>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.SCENARIOS ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.SCENARIOS, () => {
                prepareTreeRuleNavigation(RuleType.SCENARIOS);
              })}
            >
              {t('ruleManage.scenariosConfig')}
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.WPL ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.WPL, async () => {
                try {
                  await loadRepoFilesIfNeeded(RuleType.WPL);
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
              className={`side-item ${activeKey === RuleType.OML ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.OML, async () => {
                try {
                  prepareTreeRuleNavigation(RuleType.OML);
                } catch (error) {
                  message.error(t('ruleManage.loadOmlFailed', { message: error.message }));
                }
              })}
            >
              {t('ruleManage.omlConfig')}
            </button>
            <button
              type="button"
              className={`side-item ${activeKey === RuleType.KNOWLEDGE ? 'is-active' : ''}`}
              onClick={() => handleNavigation(RuleType.KNOWLEDGE, async () => {
                try {
                  const result = await fetchRuleFiles({
                    type: RuleType.KNOWLEDGE,
                    page: 1,
                    pageSize: KNOWLEDGE_PAGE_SIZE,
                    keyword: knowledgeSearch || undefined,
                  });
                  applyRuleFilesMeta(result?.meta);
                  const datasets = Array.isArray(result?.items) ? result.items : [];
                  const nextKnowledgeConfigFile =
                    result?.meta?.knowledgeConfigFile || knowledgeConfigFile;
                  const normalizedDatasets = datasets.filter(
                    (item) => item !== nextKnowledgeConfigFile,
                  );
                  setKnowledgeDatasets(normalizedDatasets);
                  setKnowledgeTotal(result?.total || normalizedDatasets.length);
                  const nextActive = normalizedDatasets.includes(activeKnowledgeDataset)
                    ? activeKnowledgeDataset
                    : nextKnowledgeConfigFile;
                  setActiveKnowledgeDataset(nextActive);
                  if (nextActive !== nextKnowledgeConfigFile && !normalizedDatasets.length) {
                    setKnowledgeDatasetConfig({ ...EMPTY_KNOWLEDGE_DATASET });
                  }
                  setKnowledgePage(result?.page || 1);
                } catch (error) {
                  message.error(t('ruleManage.loadKnowledgeFailed', { message: error.message }));
                }
              })}
            >
              {t('ruleManage.knowledgeConfig')}
            </button>
          </>
        )}
      </aside>

      {/* 右侧配置内容区 */}
      <section className="page-panels">
        <article className="panel is-visible">
          <header className="panel-header">
            <h2>{getPageTitle()}</h2>
          </header>
          <section className="panel-body config-body">
            {activeKey === RuleType.KNOWLEDGE ? (
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
                            applyRuleFilesMeta(result?.meta);
                            const datasets = Array.isArray(result?.items)
                              ? result.items
                              : [];
                            const normalized = datasets.filter(
                              (item) => item !== knowledgeConfigFile,
                            );
                            setKnowledgeDatasets(normalized);
                            setKnowledgeTotal(result?.total || normalized.length);
                            if (
                              activeKnowledgeDataset !== knowledgeConfigFile &&
                              !normalized.includes(activeKnowledgeDataset)
                            ) {
                              const nextActive = normalized[0] || knowledgeConfigFile;
                              setActiveKnowledgeDataset(nextActive);
                              if (nextActive === knowledgeConfigFile) {
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
                      const isConfigEntry = dataset === knowledgeConfigFile;
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
                            {isConfigEntry ? knowledgeConfigFile : dataset}
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
                                      applyRuleFilesMeta(refreshed?.meta);
                                      const datasets = Array.isArray(refreshed?.items)
                                        ? refreshed.items
                                        : [];
                                      const normalized = datasets.filter(
                                        (item) => item !== knowledgeConfigFile,
                                      );
                                      setKnowledgeDatasets(normalized);
                                      setKnowledgeTotal(refreshed?.total || normalized.length);
                                      if (filename === activeKnowledgeDataset) {
                                        const nextActive = normalized[0] || knowledgeConfigFile;
                                        setActiveKnowledgeDataset(nextActive);
                                        if (nextActive !== knowledgeConfigFile && !normalized.length) {
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
                              applyRuleFilesMeta(result?.meta);
                              const datasets = Array.isArray(result?.items)
                                ? result.items
                                : [];
                              const normalized = datasets.filter(
                                (item) => item !== knowledgeConfigFile,
                              );
                              setKnowledgeDatasets(normalized);
                              setKnowledgeTotal(result?.total || normalized.length);
                              if (
                                activeKnowledgeDataset !== knowledgeConfigFile &&
                                !normalized.includes(activeKnowledgeDataset)
                              ) {
                                const nextActive = normalized[0] || knowledgeConfigFile;
                                setActiveKnowledgeDataset(nextActive);
                                if (nextActive === knowledgeConfigFile) {
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
                          ? activeKnowledgeDataset === knowledgeConfigFile
                            ? knowledgeConfigFile
                            : t('ruleManage.datasetLabel', { name: activeKnowledgeDataset })
                          : t('ruleManage.datasets')}
                      </span>
                      <div className="editor-actions">
                        {showFormatButton ? (
                          <button type="button" className="btn ghost" onClick={handleFormat}>
                            {t('ruleManage.format')}
                          </button>
                        ) : null}
                        <button type="button" className="btn tertiary" onClick={handleValidate}>
                          {t('ruleManage.validate')}
                        </button>
                        <button type="button" className="btn primary" onClick={handleSave}>
                          {t('ruleManage.save')}
                        </button>
                      </div>
                    </div>
                    {activeKnowledgeDataset === knowledgeConfigFile ? (
                      <div className="knowledge-block">
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
	        ) : isFixedRuleMode ? (
          <div className="repo-content">
            <div className="repo-toolbar">
              <div className="repo-path">{WFUSION_WINDOWS_FILE}</div>
              <div className="editor-actions">
                {showFormatButton ? (
                  <button type="button" className="btn ghost" onClick={handleFormat}>
                    {t('ruleManage.format')}
                  </button>
                ) : null}
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
	        ) : isRepoMode ? (
          /* wpl/oml 配置显示 repo 布局 */
          <div className="repo-layout" data-repo={activeKey}>
            <aside
              className="repo-tree"
              aria-label={isWplEditorMode ? t('ruleManage.ruleFiles') : treeAriaLabel}
            >
              <div className="repo-tree-header">
                <h3>{isWplEditorMode ? t('ruleManage.ruleFiles') : treeSectionTitle}</h3>
                <button
                  type="button"
                  className="btn ghost repo-add-btn"
                  onClick={() => showAddModal(isWplEditorMode ? RuleType.WPL : activeTreeRuleType)}
                >
                  {t('ruleManage.add')}
                </button>
              </div>
              <div style={{ padding: '4px 0 8px' }}>
                <Input
                  size="small"
                  allowClear
                  placeholder={isWplEditorMode ? t('ruleManage.searchRuleFiles') : treeSearchPlaceholder}
                  value={isWplEditorMode ? wplSearch : omlSearch}
                  onChange={(e) => {
                    const value = e.target.value;
                    if (isWplEditorMode) {
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
                        type: activeTreeRuleType || RuleType.OML,
                        keyword: value,
                        page: 1,
                        preserveActive: true,
                      })
                        .catch((error) => {
                          message.error(
                            t(getTreeRuleLoadErrorKey(activeTreeRuleType), {
                              message: error.message,
                            }),
                          );
                        });
                    }
                  }}
                />
              </div>
              <div className="repo-folder-content" style={{ paddingLeft: 0 }}>
                {isWplEditorMode
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
                            <div style={{ marginLeft: 16, marginTop: 4 }}>
                              {node.files.map((file) => (
                                <button
                                  key={file.value}
                                  type="button"
                                  className={`repo-file ${
                                    activeWplFile === file.value ? 'is-active' : ''
                                  }`}
                                  onClick={() => handleSelectWplFile(file.value)}
                                  style={{
                                    textAlign: 'left',
                                    paddingLeft: 18,
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
                      if (node.kind === 'file') {
                        const isProtectedGlobalRule = isWfusionGlobalRuleFile(
                          activeTreeRuleType,
                          node.file.value,
                        );
                        return (
                          <div
                            key={node.file.value}
                            style={{ position: 'relative' }}
                            onMouseEnter={() => setHoveredRepoFile(node.file.value)}
                            onMouseLeave={() => setHoveredRepoFile('')}
                          >
                            <button
                              type="button"
                              className={`repo-file ${
                                activeOmlFile === node.file.value ? 'is-active' : ''
                              }`}
                              onClick={() => handleSelectTreeFile(node.file.value)}
                              style={{
                                textAlign: 'left',
                                paddingRight:
                                  hoveredRepoFile === node.file.value && !isProtectedGlobalRule
                                    ? '28px'
                                    : '12px',
                                position: 'relative',
                              }}
                            >
                              {node.file.label}
                            </button>
                            {!isProtectedGlobalRule ? (
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
                                  display:
                                    hoveredRepoFile === node.file.value ? 'inline-flex' : 'none',
                                  alignItems: 'center',
                                  justifyContent: 'center',
                                }}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  confirmDeleteTreeRule(
                                    activeTreeRuleType || RuleType.OML,
                                    node.file.value,
                                  );
                                }}
                              >
                                -
                              </button>
                            ) : null}
                          </div>
                        );
                      }

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
                            <div style={{ marginLeft: 16, marginTop: 4 }}>
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
                                    onClick={() => handleSelectTreeFile(file.value)}
                                    style={{
                                      textAlign: 'left',
                                      paddingLeft: 20,
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
                                      confirmDeleteTreeRule(
                                        activeTreeRuleType || RuleType.OML,
                                        file.value,
                                      );
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
              {(isWplEditorMode
                ? wplTotal > WPL_PAGE_SIZE
                : omlTotal > omlPageSize) ? (
                <div className="repo-pagination">
                  <Pagination
                    size="small"
                    simple
                    current={isWplEditorMode ? wplPage : omlPage}
                    pageSize={isWplEditorMode ? WPL_PAGE_SIZE : omlPageSize}
                    total={isWplEditorMode ? wplTotal : omlTotal}
                    showSizeChanger={false}
                    onChange={(page) => {
                      if (isWplEditorMode) {
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
                        type: activeTreeRuleType || RuleType.OML,
                        page,
                        preserveActive: true,
                      })
                        .catch((error) => {
                          message.error(
                            t(getTreeRuleLoadErrorKey(activeTreeRuleType), {
                              message: error.message,
                            }),
                          );
                        });
                    }}
                  />
                </div>
              ) : null}
            </aside>

	            <div className="repo-content">
	              <div className="repo-toolbar">
	                <div className="repo-path">
	                  {isWplEditorMode
	                    ? activeWplFile
	                      ? formatWplDisplayName(activeWplFile, wplParseFile)
	                      : t('ruleManage.noFileSelected')
	                    : activeOmlFile
	                      ? activeTreeRuleType === RuleType.OML
	                        ? formatOmlDisplayName(activeOmlFile)
	                        : formatNamedRuleDisplayName(activeOmlFile)
	                      : t('ruleManage.noFileSelected')}
	                </div>
	                <div className="editor-actions">
	                  {showFormatButton ? (
	                    <button type="button" className="btn ghost" onClick={handleFormat}>
	                      {t('ruleManage.format')}
	                    </button>
	                  ) : null}
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
	            {isIntegrationOverviewMode ? (
	              <aside className="repo-overview" aria-label={t('ruleManage.integrationOverview')}>
	                <div className="repo-overview-header">
	                  <h3>{t('ruleManage.integrationOverview')}</h3>
	                </div>
	                <div className="repo-overview-summary">
	                  <div className="repo-overview-stat">
	                    <span className="repo-overview-stat-label">
	                      {t('ruleManage.coveredDeviceTypes')}
	                    </span>
	                    <span className="repo-overview-stat-value">
	                      {wplOverviewStats.deviceTypeCount}
	                    </span>
	                  </div>
	                  <div className="repo-overview-stat">
	                    <span className="repo-overview-stat-label">
	                      {t('ruleManage.coveredLogTypes')}
	                    </span>
	                    <span className="repo-overview-stat-value">
	                      {wplOverviewStats.logTypeCount}
	                    </span>
	                  </div>
	                </div>
	                <div className="repo-overview-list">
	                  {wplOverviewLoading ? (
	                    <div className="repo-overview-empty">{t('common.loading')}</div>
	                  ) : wplOverviewItems.length > 0 ? (
	                    wplOverviewItems.map((item) => {
	                      const expanded = wplOverviewExpandedDevices.includes(item.packageKey);
	                      const isActiveDevice =
                          getWplEntryParts(activeWplFile, wplParseFile).rule === item.packageKey;
	                      return (
	                        <section key={item.packageKey} className="repo-overview-group">
	                          <button
	                            type="button"
	                            className={`repo-overview-device ${isActiveDevice ? 'is-active' : ''}`}
	                            onClick={() => {
	                              toggleWplOverviewDevice(item.packageKey);
	                              handleSelectWplFile(`${item.packageKey}/${wplParseFile}`);
	                            }}
	                          >
	                            <span className="repo-overview-device-title">
	                              <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
	                              <span>{item.deviceType}</span>
	                            </span>
	                            <span className="repo-overview-device-count">
	                              {item.logTypes.length}
	                            </span>
	                          </button>
	                          {expanded ? (
	                            <div className="repo-overview-log-types">
	                              {item.logTypes.map((logType) => (
	                                <button
	                                  key={`${item.packageKey}-${logType.ruleKey}`}
	                                  type="button"
	                                  className={`repo-overview-log-type ${
	                                    isActiveDevice && activeOverviewRuleKey === logType.ruleKey
	                                      ? 'is-active'
	                                      : ''
	                                  }`}
	                                  onClick={() =>
	                                    handleSelectWplFile(`${item.packageKey}/${wplParseFile}`, {
	                                      ruleKey: logType.ruleKey,
	                                    })
	                                  }
	                                >
	                                  {logType.logTypeName}
	                                </button>
	                              ))}
	                            </div>
	                          ) : null}
	                        </section>
	                      );
	                    })
	                  ) : (
	                    <div className="repo-overview-empty">
	                      {t('ruleManage.integrationOverviewEmpty')}
	                    </div>
	                  )}
	                </div>
	              </aside>
	            ) : null}
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
        destroyOnHidden
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

      {/* 校验结果弹窗 */}
      <ValidateResultModal
        open={validateModalVisible}
        onClose={() => {
          setValidateModalVisible(false);
          setValidateResult(null);
        }}
        result={validateResult}
      />
    </>
  );
}

export default RuleManagePage;
