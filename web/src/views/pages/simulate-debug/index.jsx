import { App as AntdApp, Button, Dropdown, Table, Modal, Select, Space } from 'antd';
import { RobotOutlined, UserOutlined, DownOutlined } from '@ant-design/icons';
import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router-dom';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism';
import {
  convertRecord,
  fetchDebugExamples,
  parseLogs,
  wplCodeFormat,
  omlCodeFormat,
  base64Decode,
} from '@/services/debug';
import { RuleType, fetchRuleFiles, fetchRuleConfig, saveRuleConfig, executeKnowledgeSql } from '@/services/config';
import CodeEditor from '@/views/components/CodeEditor';
import { useWorkspace } from '@/hooks/useWorkspace';
import { useMultipleInstances, createDefaultInstance } from '@/hooks/useMultipleInstances';
import InstanceSelector from '@/views/components/InstanceSelector';
import { useAssistTask } from '@/contexts/AssistTaskContext';
import AssistResultDrawer from './components/AssistResultDrawer';
import ManualTicketModal from './components/ManualTicketModal';

/**
 * 模拟调试页面
 * 功能：
 * 1. 日志解析
 * 2. 记录转换
 * 3. 知识库状态查询
 * 4. 性能测试
 * 对应原型：pages/views/simulate-debug/simulate-parse.html
 */

// 默认示例列表，保存在前端，便于无网络或后端无数据时直接展示
const DEFAULT_EXAMPLES = [
  {
    name: 'nginx',
    wpl_code:
      'package /nginx/ {\n    rule nginx {\n        (\n            ip:sip,2*_,chars:timestamp<[,]>,http/request",chars:status,chars:size,chars:referer",http/agent",_"\n        )\n    }\n}\n',
    oml_code: '',
    sample_data:
      '180.57.30.148 - - [21/Jan/2025:01:40:02 +0800] "GET /nginx-logo.png HTTP/1.1" 500 368 "<http://207.131.38.110/>" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/75.0.3770.142 Safari/537.36" "-"',
  },
];

const buildKnowledgeDefaultSql = (tableName) =>
  tableName ? `select * from ${tableName} limit 20;` : '';

const buildTypedName = (i18nT, type, number) => {
  const prefix = i18nT(`multipleInstances.type.${type}`);
  const useNoSpace = type === 'log' && /[\u4e00-\u9fff]/.test(prefix);
  const spacer = useNoSpace ? '' : ' ';
  return `${prefix}${spacer}${number}`;
};

const shouldNormalizeTypedName = (name) => {
  if (!name) return true;
  if (name.includes('{number}') || name.includes('{{number}}')) return true;
  if (/^(实例|Instance)\s*\d+$/.test(name)) return true;
  if (/^(日志)\d+$/.test(name)) return true;
  if (/^(log|wpl|oml)\s*\d+$/i.test(name)) return true;
  return false;
};

const createLogInstance = (instanceNumber, i18nT) => {
  const instance = createDefaultInstance(instanceNumber, i18nT);
  return {
    ...instance,
    name: buildTypedName(i18nT, 'log', instanceNumber),
    wpl: '',
    oml: '',
  };
};

const createWplInstance = (instanceNumber, i18nT) => {
  const instance = createDefaultInstance(instanceNumber, i18nT);
  return {
    ...instance,
    name: buildTypedName(i18nT, 'wpl', instanceNumber),
    log: '',
    oml: '',
  };
};

const createOmlInstance = (instanceNumber, i18nT) => {
  const instance = createDefaultInstance(instanceNumber, i18nT);
  return {
    ...instance,
    name: buildTypedName(i18nT, 'oml', instanceNumber),
    log: '',
    wpl: '',
  };
};

function SimulateDebugPage() {
  const { t } = useTranslation();
  const { message } = AntdApp.useApp();
  const [searchParams, setSearchParams] = useSearchParams();

  // AI 辅助任务全局 context
  const { submitTask, getTaskById } = useAssistTask();

  // AI 辅助抽屉状态（从 URL query param 或用户操作触发打开）
  const [assistDrawerOpen, setAssistDrawerOpen] = useState(false);
  const [assistDrawerTask, setAssistDrawerTask] = useState(null);

  // 人工提单弹窗状态
  const [manualModalOpen, setManualModalOpen] = useState(false);

  // 监听 URL query param assistTaskId，自动打开对应任务的结果抽屉
  useEffect(() => {
    const taskId = searchParams.get('assistTaskId');
    if (taskId) {
      const task = getTaskById(taskId);
      if (task) {
        setAssistDrawerTask(task);
        setAssistDrawerOpen(true);

        // 清除 URL 中的 assistTaskId，避免刷新后重复触发
        setSearchParams((prev) => {
          const next = new URLSearchParams(prev);
          next.delete('assistTaskId');
          return next;
        });
      }
    }
  }, [searchParams, getTaskById, setSearchParams]);

  /**
   * 提交 AI 分析任务
   * 始终以 both 模式提交，同时分析 WPL 和 OML，将当前两份规则拼接后传给 AI 参考
   */
  const handleAssistAi = async () => {
    // 将当前 WPL 和 OML 拼接为参考规则
    const wplPart = ruleValue ? `--- WPL ---\n${ruleValue}` : '';
    const omlPart = transformOml ? `--- OML ---\n${transformOml}` : '';
    const currentRule = [wplPart, omlPart].filter(Boolean).join('\n\n') || undefined;

    try {
      await submitTask({
        taskType: 'ai',
        targetRule: 'both',
        logData: inputValue,
        currentRule,
      });
      message.success(t('assistTask.aiSubmitQueued'));
    } catch {
      message.error(t('assistTask.submitFailed'));
    }
  };

  /**
   * 打开人工提单弹窗
   */
  const handleAssistManual = () => {
    setManualModalOpen(true);
  };

  /**
   * 提交人工提单
   */
  const handleManualSubmit = async (options) => {
    setManualModalOpen(false);
    try {
      await submitTask({
        taskType: 'manual',
        targetRule: options.targetRule,
        logData: options.logData,
        currentRule: options.currentRule || undefined,
        extraNote: options.extraNote || undefined,
      });
      message.success(t('assistTask.manualSubmitQueued'));
    } catch (error) {
      message.error(t('assistTask.submitFailed'));
    }
  };

  /**
   * 一键填入全部区域并执行解析和转换
   * 将 AI 建议的 WPL/OML 分别填入对应编辑区，然后自动触发解析，解析完成后再触发转换
   * @param {Object} task - 辅助任务对象
   */
  const handleFillAll = async (task) => {
    const wplCode = task?.wpl_suggestion || null;
    const omlCode = task?.oml_suggestion || null;
    const logData = task?.log_data || '';

    setAssistDrawerOpen(false);

    // 将任务提交时的日志、WPL 和 OML 一并回填，确保页面恢复到该任务的分析上下文
    if (logData) setInputValue(logData);
    if (wplCode) setRuleValue(wplCode);
    if (omlCode) setTransformOml(omlCode);

    // 触发解析（优先使用任务原始日志和新填入的 WPL，若没有则退回当前值）
    const logsToUse = logData || inputValue;
    const wplToUse = wplCode || ruleValue;
    if (!logsToUse || !wplToUse) {
      message.success(t('assistTask.fillSuccess'));
      return;
    }

    setLoading(true);
    setParseError(null);
    try {
      const parseResponse = await parseLogs({ logs: logsToUse, rules: wplToUse });
      setResult(parseResponse);

      let parseResultForTransform = null;
      if (parseResponse?.fields) {
        parseResultForTransform = { fields: parseResponse.fields, formatJson: parseResponse.formatJson };
        setTransformParseResult(parseResultForTransform);
      }

      // 解析完成后自动触发转换（用新填入的 OML，若没有则用当前值）
      const omlToUse = omlCode || transformOml;
      if (omlToUse && parseResultForTransform) {
        const convertResponse = await convertRecord({ oml: omlToUse, parseResult: parseResultForTransform });
        let fieldsData = [];
        if (Array.isArray(convertResponse?.fields)) {
          fieldsData = convertResponse.fields;
        } else if (convertResponse?.fields && Array.isArray(convertResponse?.fields?.items)) {
          fieldsData = convertResponse.fields.items;
        }
        setTransformResult({
          fields: processFieldsForDisplay(fieldsData, convertResponse.formatJson || ''),
          formatJson: convertResponse.formatJson || '',
          rawFields: fieldsData,
        });
        setTransformError(null);
      }

      message.success(t('assistTask.fillSuccess'));
    } catch (error) {
      setParseError(error);
      message.error(t('assistTask.fillFailed'));
    } finally {
      setLoading(false);
    }
  };

  // 工作区管理
  const {
    workspaceMode,
    workspaceData,
    saveWorkspace,
    updateWorkspace,
    clearWorkspace,
    switchMode,
  } = useWorkspace();
  
  // 多实例管理（日志/WPL/OML 分离）
  const {
    instances: logInstances,
    activeInstanceIndex: activeLogIndex,
    activeInstance: activeLogInstance,
    addInstance: addLogInstance,
    removeInstance: removeLogInstance,
    switchInstance: switchLogInstance,
    renameInstance: renameLogInstance,
    updateActiveInstance: updateActiveLogInstance,
    clearAllInstances: clearAllLogInstances,
    saveToStorage: saveLogInstances,
    restoreFromStorage: restoreLogInstances,
  } = useMultipleInstances({
    storageKey: 'warpparse_multiple_instances_log',
    createDefaultInstance: createLogInstance,
    normalizeName: (instance, index, i18nT) => (
      shouldNormalizeTypedName(instance?.name) ? buildTypedName(i18nT, 'log', index + 1) : null
    ),
  });

  const {
    instances: wplInstances,
    activeInstanceIndex: activeWplIndex,
    activeInstance: activeWplInstance,
    addInstance: addWplInstance,
    removeInstance: removeWplInstance,
    switchInstance: switchWplInstance,
    renameInstance: renameWplInstance,
    updateActiveInstance: updateActiveWplInstance,
    clearAllInstances: clearAllWplInstances,
    saveToStorage: saveWplInstances,
    restoreFromStorage: restoreWplInstances,
  } = useMultipleInstances({
    storageKey: 'warpparse_multiple_instances_wpl',
    createDefaultInstance: createWplInstance,
    normalizeName: (instance, index, i18nT) => (
      shouldNormalizeTypedName(instance?.name) ? buildTypedName(i18nT, 'wpl', index + 1) : null
    ),
  });

  const {
    instances: omlInstances,
    activeInstanceIndex: activeOmlIndex,
    activeInstance: activeOmlInstance,
    addInstance: addOmlInstance,
    removeInstance: removeOmlInstance,
    switchInstance: switchOmlInstance,
    renameInstance: renameOmlInstance,
    updateActiveInstance: updateActiveOmlInstance,
    clearAllInstances: clearAllOmlInstances,
    saveToStorage: saveOmlInstances,
    restoreFromStorage: restoreOmlInstances,
  } = useMultipleInstances({
    storageKey: 'warpparse_multiple_instances_oml',
    createDefaultInstance: createOmlInstance,
    normalizeName: (instance, index, i18nT) => (
      shouldNormalizeTypedName(instance?.name) ? buildTypedName(i18nT, 'oml', index + 1) : null
    ),
  });
  
  const [activeKey, setActiveKey] = useState('parse');
  const isExamplesMode = workspaceMode === 'examples';
  
  // 示例区独立状态（避免污染工作区）
  const [exampleLog, setExampleLog] = useState('');
  const [exampleWpl, setExampleWpl] = useState('');
  const [exampleOml, setExampleOml] = useState('');
  const [exampleParseResult, setExampleParseResult] = useState(null);
  const [exampleParseError, setExampleParseError] = useState(null);
  const [exampleTransformParseResult, setExampleTransformParseResult] = useState(null);
  const [exampleTransformResult, setExampleTransformResult] = useState(null);
  const [exampleTransformError, setExampleTransformError] = useState(null);
  const [exampleSelected, setExampleSelected] = useState(null);

  // 从激活实例中提取数据（日志/WPL/OML 分离）
  const inputValue = isExamplesMode ? exampleLog : activeLogInstance.log;
  const setInputValue = (value) => (
    isExamplesMode ? setExampleLog(value) : updateActiveLogInstance({ log: value })
  );
  const ruleValue = isExamplesMode ? exampleWpl : activeWplInstance.wpl;
  const setRuleValue = (value) => (
    isExamplesMode ? setExampleWpl(value) : updateActiveWplInstance({ wpl: value })
  );
  const result = isExamplesMode ? exampleParseResult : activeLogInstance.parseResult;
  const setResult = (value) => (
    isExamplesMode ? setExampleParseResult(value) : updateActiveLogInstance({ parseResult: value })
  );
  const parseError = isExamplesMode ? exampleParseError : activeLogInstance.parseError;
  const setParseError = (value) => (
    isExamplesMode ? setExampleParseError(value) : updateActiveLogInstance({ parseError: value })
  );
  const transformOml = isExamplesMode ? exampleOml : activeOmlInstance.oml;
  const setTransformOml = (value) => (
    isExamplesMode ? setExampleOml(value) : updateActiveOmlInstance({ oml: value })
  );
  const transformParseResult = isExamplesMode ? exampleTransformParseResult : activeLogInstance.transformParseResult;
  const setTransformParseResult = (value) => (
    isExamplesMode
      ? setExampleTransformParseResult(value)
      : updateActiveLogInstance({ transformParseResult: value })
  );
  const transformResult = isExamplesMode ? exampleTransformResult : activeOmlInstance.transformResult;
  const setTransformResult = (value) => (
    isExamplesMode ? setExampleTransformResult(value) : updateActiveOmlInstance({ transformResult: value })
  );
  const transformError = isExamplesMode ? exampleTransformError : activeOmlInstance.transformError;
  const setTransformError = (value) => (
    isExamplesMode ? setExampleTransformError(value) : updateActiveOmlInstance({ transformError: value })
  );
  const selectedExample = isExamplesMode ? exampleSelected : activeLogInstance.selectedExample;
  const setSelectedExample = (value) => (
    isExamplesMode ? setExampleSelected(value) : updateActiveLogInstance({ selectedExample: value })
  );
  
  const [loading, setLoading] = useState(false);
  const [viewMode, setViewMode] = useState('table');
  // 解析页“显示空值”开关
  const [showEmpty, setShowEmpty] = useState(true);


  // 转换相关状态
  const [transformParseViewMode, setTransformParseViewMode] = useState('table');
  const [transformResultViewMode, setTransformResultViewMode] = useState('table');
  // 转换页"显示空值"开关（转换结果）
  const [transformResultShowEmpty, setTransformResultShowEmpty] = useState(true);
  // 转换页"显示空值"开关（解析结果）
  const [transformParseShowEmpty, setTransformParseShowEmpty] = useState(true);
  
  // 示例列表状态
  const [examples, setExamples] = useState(DEFAULT_EXAMPLES);
  const examplesOpen = true;
  const [examplesLoading, setExamplesLoading] = useState(false);
  const [examplesLoaded, setExamplesLoaded] = useState(false);
  const examplesFetchedRef = useRef(false); // 防止严格模式导致的重复请求

  // 知识库相关状态
  const [knowledgeDatasets, setKnowledgeDatasets] = useState([]);
  const [knowledgeTable, setKnowledgeTable] = useState('');
  const [knowledgeSql, setKnowledgeSql] = useState('');
  const [knowledgeResult, setKnowledgeResult] = useState(null);
  const [knowledgeViewMode, setKnowledgeViewMode] = useState('table');
  const [knowledgeLoading, setKnowledgeLoading] = useState(false);

  // 性能测试相关状态
  const EXAMPLE_LOG = `222.133.52.20 - - [06/Aug/2019:12:12:19 +0800] "GET /nginx-logo.png HTTP/1.1" 200 368 "http://119.122.1.4/" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/75.0.3770.142 Safari/537.36" "-"`;
  const [performanceSample, setPerformanceSample] = useState(EXAMPLE_LOG);
  const [performanceConfig, setPerformanceConfig] = useState(`version = "1.0"

[main_conf]
gen_ref = "sample_gen"
gen_speed = 100000
gen_count = 1000000
gen_secs = 0
gen_parallel = 1
out_ref = "out_file"

[main_conf.log_conf]
level = "warn,ctrl=info,launch=info,klib=info"
output = "Console"
output_path = "./logs/"`);
  const [performanceResult, setPerformanceResult] = useState(null);

  // 规则文件管理状态
  const [wplModalVisible, setWplModalVisible] = useState(false);
  const [wplFiles, setWplFiles] = useState([]);
  const [selectedWplFile, setSelectedWplFile] = useState('');
  const [currentWplFile, setCurrentWplFile] = useState('');
  const [omlModalVisible, setOmlModalVisible] = useState(false);
  const [omlFiles, setOmlFiles] = useState([]);
  const [selectedOmlFile, setSelectedOmlFile] = useState('');
  const [currentOmlFile, setCurrentOmlFile] = useState('');

  const formatJsonForDisplay = (formatJson, fallbackData, postProcess) => {
    if (formatJson) {
      try {
        const parsed = JSON.parse(formatJson);
        const processed = postProcess ? postProcess(parsed) : parsed;
        return JSON.stringify(processed, null, 2);
      } catch (_e) {
        return formatJson;
      }
    }
    return JSON.stringify(fallbackData, null, 2);
  };

  /**
   * 处理测试/解析按钮点击
   * 调用服务层解析日志
   */
  const handleTest = async () => {
    setLoading(true);
    setParseError(null); // 重置错误状态
    try {
      // 调用服务层解析日志（使用对象参数）
      const response = await parseLogs({
        logs: inputValue,
        rules: ruleValue,
      });
      setResult(response);
      // 同步更新转换页的解析结果（使用原始数据用于转换）
      if (response?.fields) {
        setTransformParseResult({ fields: response.fields, formatJson: response.formatJson });
      }
    } catch (error) {
      setParseError(error); // 将错误存储到状态中
    } finally {
      setLoading(false);
    }
  };

  /**
   * 展示示例列表，按需拉取数据（默认展开，不再折叠）
   */
  const handleToggleExamples = async () => {
    if (examplesLoading || examplesLoaded || examplesFetchedRef.current) {
      return;
    }
    examplesFetchedRef.current = true;
    // 拉取示例列表，供用户选择
    setExamplesLoading(true);
    try {
      const data = await fetchDebugExamples();
      const list = data && typeof data === 'object' ? Object.values(data) : [];
      if (Array.isArray(list) && list.length > 0) {
        setExamples(list);
        setExamplesLoaded(true);
      }
      // 移除了空列表时的提示信息
    } catch (error) {
      message.error(`${t('simulateDebug.examples.fetchError')}：${error?.message || error}`);
    } finally {
      setExamplesLoading(false);
    }
  };

  const wplFormat = async () => {
    try {
      const response = await wplCodeFormat(ruleValue);
      const formattedWpl = response?.wpl_code || '';
      setRuleValue(formattedWpl);
      setParseError(null);
    } catch (error) {
      const detail = error?.responseData?.error?.detail;
      const baseMessage = error?.responseData?.error?.message || error?.message || error;
      const fullMessage = detail || baseMessage;
      const err = new Error(fullMessage);
      err.code = error?.code || error?.responseData?.error?.code;
      err.responseData = error?.responseData;
      setParseError(err);
      setResult(null);
    }
  };

  // 监听输入变化，在工作区模式下更新工作区数据
  useEffect(() => {
    if (workspaceMode === 'workspace') {
      // 保存所有实例到工作区
      updateWorkspace({
        logInstances: logInstances,
        logActiveIndex: activeLogIndex,
        wplInstances: wplInstances,
        wplActiveIndex: activeWplIndex,
        omlInstances: omlInstances,
        omlActiveIndex: activeOmlIndex,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logInstances, activeLogIndex, wplInstances, activeWplIndex, omlInstances, activeOmlIndex, workspaceMode]);

  // 页面加载后默认展开示例并尝试拉取
  useEffect(() => {
    handleToggleExamples();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /**
   * 应用某个示例到日志、规则与 OML 输入区域，并自动尝试解析
   */
  const handleApplyExample = async exampleItem => {
    if (!exampleItem) return;
    const { sample_data: sampleData, wpl_code: wplCode, oml_code: omlCode } = exampleItem;
    
    setExampleLog(sampleData || '');
    setExampleWpl(wplCode || '');
    setExampleOml(omlCode || '');
    setExampleSelected(exampleItem.name); // 更新选中的示例

    if (!sampleData || !wplCode) {
      return;
    }

    setLoading(true);
    setExampleParseError(null);
    try {
      const response = await parseLogs({
        logs: sampleData,
        rules: wplCode,
      });
      setExampleParseResult(response);
      if (response?.fields) {
        setExampleTransformParseResult({ fields: response.fields, formatJson: response.formatJson });
      }
    } catch (error) {
      setExampleParseError(error);
    } finally {
      setLoading(false);
    }
  };

  /**
   * 一键清空
   * 清空解析和转换的所有输入和结果
   */
  const handleClear = () => {
    if (workspaceMode === 'examples') {
      setExampleLog('');
      setExampleWpl('');
      setExampleOml('');
      setExampleParseResult(null);
      setExampleParseError(null);
      setExampleTransformParseResult(null);
      setExampleTransformResult(null);
      setExampleTransformError(null);
      setExampleSelected(null);
      return;
    }

    // 使用 clearAllInstances 清空所有实例
    clearAllLogInstances();
    clearAllWplInstances();
    clearAllOmlInstances();
    
    // 如果在工作区模式，也清空工作区数据（包括解析结果）
    if (workspaceMode === 'workspace') {
      clearWorkspace();
    }
  };
  
  /**
   * 切换工作区/示例区
   */
  const handleSwitchMode = (mode) => {
    if (workspaceMode === 'workspace' && mode === 'examples') {
      saveLogInstances();
      saveWplInstances();
      saveOmlInstances();
    }

    // 保存所有实例数据
    const currentData = {
      logInstances: logInstances,
      logActiveIndex: activeLogIndex,
      wplInstances: wplInstances,
      wplActiveIndex: activeWplIndex,
      omlInstances: omlInstances,
      omlActiveIndex: activeOmlIndex,
    };
    
    const loadedData = switchMode(mode, currentData);
    
    if (mode === 'workspace') {
      restoreLogInstances();
      restoreWplInstances();
      restoreOmlInstances();
    }

    if (mode === 'workspace' && loadedData) {
      // 切换回工作区，恢复所有实例
      // 注意：实例数据已经通过 useMultipleInstances 持久化到 localStorage
      // 这里只需要显示提示消息
      message.success(t('simulateDebug.workspace.loadSuccess'));
    } else if (mode === 'examples') {
      // 切换到示例区
      message.success(t('simulateDebug.workspace.autoSaved'));
    }
  };

  // 处理 Base64 解码按钮点击
  const handleBase64Decode = async () => {
    try {
      const response = await base64Decode(inputValue);
      const decodedValue = response || '';
      setInputValue(decodedValue);
    } catch (error) {
      message.error(`${t('simulateDebug.logData.base64Error')}`);
    }
  };

  const handleTransform = async () => {
    if (!transformOml) {
      message.warning(t('simulateDebug.omlInput.fillOmlWarning'));
      return;
    }
    setLoading(true);
    setTransformError(null);
    try {
      const response = await convertRecord({
        oml: transformOml,
        parseResult: transformParseResult,
      });
      let fieldsData = [];
      if (Array.isArray(response?.fields)) {
        fieldsData = response.fields;
      } else if (response?.fields && Array.isArray(response?.fields?.items)) {
        fieldsData = response.fields.items;
      }
      const processedFields = processFieldsForDisplay(fieldsData, response.formatJson || '');
      setTransformResult({
        fields: processedFields,
        formatJson: response.formatJson || '',
        rawFields: fieldsData,
      });
      setTransformError(null);
    } catch (error) {
      message.error(`${t('simulateDebug.convertResult.convertError')}：${error?.message || error}`);
      setTransformError(error);
      setTransformResult(null);
    } finally {
      setLoading(false);
    }
  };

  const omlFormat = async () => {
    try {
      const response = await omlCodeFormat(transformOml);
      const formattedOml = response?.oml_code || '';
      setTransformOml(formattedOml);
      setTransformError(null);
    } catch (error) {
      const detail = error?.responseData?.error?.detail;
      const baseMessage = error?.responseData?.error?.message || error?.message || error;
      const fullMessage = detail || baseMessage;
      const err = new Error(fullMessage);
      err.code = error?.code || error?.responseData?.error?.code;
      err.responseData = error?.responseData;
      setTransformError(err);
      setTransformResult(null);
    }
  };

  /**
   * 加载知识库数据集列表
   */
  useEffect(() => {
    const loadKnowledgeDatasets = async () => {
      try {
        const result = await fetchRuleFiles({ type: RuleType.KNOWLEDGE });
        const datasets = Array.isArray(result?.items) ? result.items : [];
        if (datasets.length > 0) {
          setKnowledgeDatasets(datasets);
          const firstDataset = datasets[0];
          setKnowledgeTable(firstDataset);
          setKnowledgeSql(buildKnowledgeDefaultSql(firstDataset));
        }
      } catch (error) {
        message.error('加载知识库列表失败：' + error.message);
      }
    };
    loadKnowledgeDatasets();
  }, []);

  /**
   * 处理知识库表切换
   */
  const handleKnowledgeTableChange = (tableName) => {
    setKnowledgeTable(tableName);
    setKnowledgeSql(buildKnowledgeDefaultSql(tableName));
    setKnowledgeResult(null);
  };

  /**
   * 更新知识库表列表
   */
  const handleKnowledgeUpdate = async () => {
    try {
      const result = await fetchRuleFiles({ type: 'knowledge' });
      const datasets = Array.isArray(result?.items) ? result.items : [];
      if (datasets.length > 0) {
        setKnowledgeDatasets(datasets);
        message.success('更新成功');
      }
    } catch (error) {
      message.error('更新失败：' + error.message);
    }
  };

  /**
   * 执行知识库查询
   */
  const handleKnowledgeQuery = async () => {
    if (!knowledgeTable) {
      message.warning('请选择知识库表');
      return;
    }
    setKnowledgeLoading(true);
    try {
      const result = await executeKnowledgeSql(knowledgeSql);
      setKnowledgeResult(result);
      if (result.fields.length > 0) {
        message.success('查询成功');
      } else {
        message.warning('未找到数据');
      }
    } catch (error) {
      message.error('查询失败：' + error.message);
    } finally {
      setKnowledgeLoading(false);
    }
  };

  /**
   * 打开 WPL 规则加载模态框
   */
  const handleOpenWplModal = async () => {
    try {
      const resultFiles = await fetchRuleFiles({ type: RuleType.WPL });
      const items = Array.isArray(resultFiles?.items) ? resultFiles.items : resultFiles;
      const files = Array.isArray(items) ? items : [];
      setWplFiles(files);
      if (files.length > 0 && !selectedWplFile) {
        setSelectedWplFile(files[0]);
      }
      setWplModalVisible(true);
    } catch (error) {
      message.error(`加载 WPL 规则列表失败：${error?.message || error}`);
    }
  };

  /**
   * 确认加载 WPL 规则
   */
  const handleConfirmLoadWpl = async () => {
    if (!selectedWplFile) {
      message.warning('请先选择一个 WPL 规则文件');
      return;
    }
    try {
      const config = await fetchRuleConfig({ type: RuleType.WPL, file: selectedWplFile });
      setRuleValue(config?.content || '');
      setCurrentWplFile(config?.file || selectedWplFile);
      setWplModalVisible(false);
    } catch (error) {
      message.error(`加载规则内容失败：${error?.message || error}`);
    }
  };

  /**
   * 保存 WPL 规则
   */
  const handleSaveWplRule = async () => {
    if (!currentWplFile) {
      message.warning('请先通过"加载规则"选择一个 WPL 文件');
      return;
    }
    try {
      await saveRuleConfig({
        type: RuleType.WPL,
        file: currentWplFile,
        content: ruleValue || '',
      });
      message.success('保存成功');
    } catch (error) {
      message.error(`保存规则失败：${error?.message || error}`);
    }
  };

  /**
   * 打开 OML 规则加载模态框
   */
  const handleOpenOmlModal = async () => {
    try {
      const resultFiles = await fetchRuleFiles({ type: RuleType.OML });
      const items = Array.isArray(resultFiles?.items) ? resultFiles.items : resultFiles;
      const files = Array.isArray(items) ? items : [];
      setOmlFiles(files);
      if (files.length > 0 && !selectedOmlFile) {
        setSelectedOmlFile(files[0]);
      }
      setOmlModalVisible(true);
    } catch (error) {
      message.error(`加载 OML 规则列表失败：${error?.message || error}`);
    }
  };

  /**
   * 确认加载 OML 规则
   */
  const handleConfirmLoadOml = async () => {
    if (!selectedOmlFile) {
      message.warning('请先选择一个 OML 规则文件');
      return;
    }
    try {
      const config = await fetchRuleConfig({ type: RuleType.OML, file: selectedOmlFile });
      setTransformOml(config?.content || '');
      setCurrentOmlFile(config?.file || selectedOmlFile);
      setOmlModalVisible(false);
    } catch (error) {
      message.error(`加载规则内容失败：${error?.message || error}`);
    }
  };

  /**
   * 保存 OML 规则
   */
  const handleSaveOmlRule = async () => {
    if (!currentOmlFile) {
      message.warning('请先通过"加载规则"选择一个 OML 文件');
      return;
    }
    try {
      await saveRuleConfig({
        type: RuleType.OML,
        file: currentOmlFile,
        content: transformOml || '',
      });
      message.success('保存成功');
    } catch (error) {
      message.error(`保存规则失败：${error?.message || error}`);
    }
  };

  const menuItems = [
    { key: 'parse', label: t('simulateDebug.tabs.parse') },
    { key: 'convert', label: t('simulateDebug.tabs.convert') },
  ];

  const resultColumns = [
    { title: t('simulateDebug.table.no'), dataIndex: 'no', key: 'no', width: 60 },
    { title: t('simulateDebug.table.meta'), dataIndex: 'meta', key: 'meta', width: 120 },
    { title: t('simulateDebug.table.name'), dataIndex: 'name', key: 'name', width: 150 },
    {
      title: t('simulateDebug.table.value'),
      dataIndex: 'value',
      key: 'value',
      width: 300,
      render: (text) => (
        <div style={{ 
          wordWrap: 'break-word', 
          wordBreak: 'break-all', 
          maxWidth: '300px',
          whiteSpace: 'pre-wrap'
        }}>
          {text}
        </div>
      ),
    },
  ];

  /**
   * 按"显示空值"开关过滤字段列表
   * showEmptyFlag=false 时，过滤掉 value 为空字符串/null/undefined 的行
   */
  const filterFieldsByShowEmpty = (fields, showEmptyFlag) => {
    const list = Array.isArray(fields) ? fields : [];
    if (showEmptyFlag) {
      return list;
    }
    return list.filter(fieldItem => {
      if (!fieldItem || typeof fieldItem !== 'object') {
        return false;
      }
      const fieldValue = fieldItem.value;
      return fieldValue !== '' && fieldValue !== null && fieldValue !== undefined;
    });
  };

  /**
   * 过滤 JSON 对象中的空字段
   * 用于 JSON 模式下的显示空值开关
   */
  const filterEmptyFields = (obj) => {
    if (!obj || typeof obj !== 'object') {
      return obj;
    }
    
    if (Array.isArray(obj)) {
      return obj
        .map(item => filterEmptyFields(item))
        .filter(item => item !== null && item !== undefined && item !== '');
    }
    
    const filtered = {};
    Object.keys(obj).forEach(key => {
      const value = obj[key];
      if (value !== null && value !== undefined && value !== '') {
        if (typeof value === 'object') {
          const filteredValue = filterEmptyFields(value);
          if (Object.keys(filteredValue).length > 0 || Array.isArray(filteredValue)) {
            filtered[key] = filteredValue;
          }
        } else {
          filtered[key] = value;
        }
      }
    });
    
    return filtered;
  };

  /**
   * 从 value 对象中提取值
   * @param {Object} valueObj - value 对象，如 { "IpAddr": "..." } 或 { "Chars": "..." }
   * @param {string} fieldName - 字段名称
   * @param {string} formatJson - format_json 字符串
   * @returns {string} 提取的值字符串
   */
  const extractValueFromObj = (valueObj, fieldName, formatJson) => {
    if (valueObj === null || valueObj === undefined) {
      return '';
    }

    if (typeof valueObj !== 'object') {
      return String(valueObj);
    }

    // 处理普通数组
    if (Array.isArray(valueObj)) {
      const arrayValues = valueObj
        .map(item => extractValueFromObj(item, fieldName, formatJson))
        .filter(val => val !== '' && val !== null && val !== undefined);
      return arrayValues.length > 0 ? `[${arrayValues.join(', ')}]` : '';
    }

    // 处理 Array 字段（包含 meta/name/value 结构的数组）
    if (Array.isArray(valueObj.Array)) {
      const arrayValues = valueObj.Array.map(item => {
        if (item && typeof item === 'object' && 'value' in item) {
          return extractValueFromObj(item.value, fieldName, formatJson);
        }
        return extractValueFromObj(item, fieldName, formatJson);
      }).filter(val => val !== '' && val !== null && val !== undefined);
      return arrayValues.length > 0 ? `[${arrayValues.join(', ')}]` : '';
    }

    const keys = Object.keys(valueObj);
    if (keys.length === 0) {
      return '';
    }

    const firstKey = keys[0];
    const rawValue = valueObj[firstKey];

    if (rawValue === null || rawValue === undefined) {
      return '';
    }

    // 对于复杂嵌套对象（如 IpNet），尝试从 format_json 中读取
    if (typeof rawValue === 'object' && fieldName && formatJson) {
      try {
        const jsonData = JSON.parse(formatJson);
        if (jsonData && jsonData[fieldName] !== undefined) {
          return String(jsonData[fieldName]);
        }
      } catch (e) {
        // JSON 解析失败，继续使用原有逻辑
      }
    }

    if (typeof rawValue === 'object') {
      return extractValueFromObj(rawValue, fieldName, formatJson);
    }

    return String(rawValue);
  };

  /**
   * 处理需要展示的字段列表，添加 no 序号并提取 value 值
   * @param {Array} fields - 原始字段数组
   * @param {string} formatJson - format_json 字符串
   * @returns {Array} 处理后的字段数组
   */
  const processFieldsForDisplay = (fields, formatJson) => {
    const list = Array.isArray(fields) ? fields : [];
    return list.map((field, index) => {
      // 处理 meta 字段
      let metaDisplay = field.meta;
      if (field.meta && typeof field.meta === 'object') {
        if (field.meta.array) {
          // 数组类型：显示为 "array:元素类型"
          metaDisplay = `array:${field.meta.array}`;
        } else {
          // 其他对象类型：转换为 JSON 字符串
          metaDisplay = JSON.stringify(field.meta);
        }
      }

      return {
        ...field,
        no: index + 1,
        meta: metaDisplay,
        value: extractValueFromObj(field?.value, field?.name, formatJson),
      };
    });
  };

  // 统一渲染解析错误内容，仅保留错误标题和错误码提示
  const renderParseError = () => {
    if (!parseError) return null;

    return (
      <div
        style={{
          padding: '20px',
          backgroundColor: '#fff1f0',
          border: '1px solid #ffccc7',
          borderRadius: 4,
          margin: '10px',
        }}
      >
        <h4 style={{ color: '#f5222d', marginBottom: '8px', fontWeight: 'bold' }}>
          {t('simulateDebug.parseResult.parseFailed')}
        </h4>
        <pre
          style={{
            whiteSpace: 'pre-wrap',
            wordWrap: 'break-word',
            color: '#666',
            margin: '0 0 8px 0',
            fontSize: '14px',
            lineHeight: '1.5',
          }}
        >
          {parseError.message || t('simulateDebug.parseResult.parseError')}
        </pre>
        {parseError.code && (
          <p style={{ color: '#f5222d', margin: '8px 0 0 0' }}>
            <span style={{ fontWeight: 'bold' }}>{t('simulateDebug.parseResult.errorCode')}：</span>
            {parseError.code}
          </p>
        )}
      </div>
    );
  };

  // 转换错误展示，仅保留错误标题与错误码
  const renderTransformError = () => {
    if (!transformError) return null;
    const errorMessage =
      transformError.responseData?.error?.detail ||
      transformError.message ||
      transformError.responseData?.error?.message ||
      transformError.data?.error?.message ||
      t('simulateDebug.convertResult.convertError');

    return (
      <div
        style={{
          padding: '20px',
          backgroundColor: '#fff1f0',
          border: '1px solid #ffccc7',
          borderRadius: 4,
          margin: '10px',
        }}
      >
        <h4 style={{ color: '#f5222d', marginBottom: '8px', fontWeight: 'bold' }}>
          {t('simulateDebug.convertResult.convertFailed')}
        </h4>
        <pre
          style={{
            whiteSpace: 'pre-wrap',
            wordWrap: 'break-word',
            color: '#666',
            margin: '0 0 8px 0',
            fontSize: '14px',
            lineHeight: '1.5',
          }}
        >
          {errorMessage}
        </pre>
        {transformError.code && (
          <p style={{ color: '#f5222d', margin: '8px 0 0 0' }}>
            <span style={{ fontWeight: 'bold' }}>{t('simulateDebug.parseResult.errorCode')}：</span>
            {transformError.code}
          </p>
        )}
      </div>
    );
  };

  return (
    <>
      <aside className="side-nav" data-group="simulate-debug">
        <h2>{t('simulateDebug.title')}</h2>
        <button
          type="button"
          className={`side-item ${activeKey === 'parse' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('parse')}
        >
          {t('simulateDebug.tabs.parse')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'convert' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('convert')}
        >
          {t('simulateDebug.tabs.convert')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'knowledge' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('knowledge')}
        >
          {t('simulateDebug.tabs.knowledge')}
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'performance' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('performance')}
        >
          {t('simulateDebug.tabs.performance')}
        </button>

        <h2 style={{ marginTop: "20px" }}>{t('simulateDebug.workspace.mode')}</h2>
        <button
          type="button"
          className={`side-item ${workspaceMode === 'workspace' ? 'is-active' : ''}`}
          onClick={() => handleSwitchMode('workspace')}
        >
          {t('simulateDebug.workspace.title')}
        </button>
        <button
          type="button"
          className={`side-item ${workspaceMode === 'examples' ? 'is-active' : ''}`}
          onClick={() => handleSwitchMode('examples')}
        >
          {t('simulateDebug.examples.title')}
        </button>
        
        {workspaceMode === 'examples' && (
          <div className="example-list example-list--compact example-list--spaced" style={{ marginTop: "10px" }}>
            <div className="example-list__header">
              <div>
                <h4 className="example-list__title">{t('simulateDebug.examples.title')}</h4>
                <p className="example-list__desc">{t('simulateDebug.examples.desc')}</p>
              </div>
            </div>
            {examplesLoading ? (
              <div className="example-list__message">{t('simulateDebug.examples.loading')}</div>
            ) : examples && examples.length > 0 ? (
              <div className="example-list__grid example-list__grid--small">
                {examples.map(exampleItem => (
                  <button
                    key={exampleItem.name || exampleItem.key}
                    type="button"
                    className={`example-list__item ${selectedExample === exampleItem.name ? 'is-active' : ''}`}
                    onClick={() => handleApplyExample(exampleItem)}
                  >
                    {exampleItem.name || t('simulateDebug.examples.unnamed')}
                  </button>
                ))}
              </div>
            ) : (
              <div className="example-list__message">{t('simulateDebug.examples.noData')}</div>
            )}
          </div>
        )}
      </aside>

      <section className="page-panels">
        <article className="panel is-visible">
          <section className="panel-body simulate-body">
            {/* 解析页面 */}
            {activeKey === 'parse' && (
              <>
                <div className="panel-block">
                  <div className="block-header" style={{ flexWrap: 'nowrap', alignItems: 'center' }}>
                    <div style={{ minWidth: 0, flex: 1, display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                      <h3>{t('simulateDebug.logData.title')}</h3>
                      {workspaceMode === 'workspace' && (
                        <InstanceSelector
                          instances={logInstances}
                          activeIndex={activeLogIndex}
                          maxInstances={10}
                          onSwitch={switchLogInstance}
                          onAdd={addLogInstance}
                          onRemove={removeLogInstance}
                          onRename={renameLogInstance}
                          inline
                          showAddButton={false}
                          collapseThreshold={6}
                        />
                      )}
                    </div>
                    <div
                      className="block-actions"
                      style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'nowrap', minWidth: 0 }}
                    >
                      {workspaceMode === 'workspace' && (
                        <button
                          type="button"
                          className="btn primary"
                          onClick={addLogInstance}
                          disabled={logInstances.length >= 10}
                          title={logInstances.length >= 10
                            ? t('multipleInstances.maxInstancesReached')
                            : t('multipleInstances.addInstance')}
                        >
                          {t('multipleInstances.addInstance')}
                        </button>
                      )}
                      <button type="button" className="btn ghost" onClick={handleBase64Decode}>
                        {t('simulateDebug.logData.base64Decode')}
                      </button>
                      <button type="button" className="btn ghost" onClick={handleClear}>
                        {t('simulateDebug.logData.clearAll')}
                      </button>
                    </div>
                  </div>
                <CodeEditor
                  key={`log-${workspaceMode}-${activeLogInstance?.id || activeLogIndex}`}
                  className="code-area"
                  language="json"
                  theme="vscodeDark"
                  value={inputValue}
                  onChange={value => setInputValue(value)}
                />
                </div>

                <div className="split-layout">
                  <div className="split-col">
                    <div className="panel-block panel-block--fill">
                      <div className="block-header" style={{ flexWrap: 'nowrap', alignItems: 'center' }}>
                        <div style={{ minWidth: 0, flex: 1, display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                          <h3>{t('simulateDebug.parseRule.title')}</h3>
                          {workspaceMode === 'workspace' && (
                            <InstanceSelector
                              instances={wplInstances}
                              activeIndex={activeWplIndex}
                              maxInstances={10}
                              onSwitch={switchWplInstance}
                              onAdd={addWplInstance}
                              onRemove={removeWplInstance}
                              onRename={renameWplInstance}
                              inline
                              inlineMaxWidth="400px"
                              showAddButton={false}
                              collapseThreshold={6}
                            />
                          )}
                        </div>
                        <div className="block-actions" style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'nowrap', minWidth: 0 }}>
                          {workspaceMode === 'workspace' && (
                            <button
                              type="button"
                              className="btn primary"
                              onClick={addWplInstance}
                              disabled={wplInstances.length >= 10}
                              title={wplInstances.length >= 10
                                ? t('multipleInstances.maxInstancesReached')
                                : t('multipleInstances.addInstance')}
                            >
                              {t('multipleInstances.addInstance')}
                            </button>
                          )}
                          <button type="button" className="btn ghost" onClick={wplFormat}>
                            {t('simulateDebug.parseRule.format')}
                          </button>
                          <button
                            type="button"
                            className="btn primary"
                            onClick={handleTest}
                            disabled={loading}
                          >
                            {loading
                              ? t('simulateDebug.parseRule.parsing')
                              : t('simulateDebug.parseRule.parse')}
                          </button>
                          {/* AI 辅助入口：AI 分析 / 人工提单 */}
                          <Dropdown
                            menu={{
                              items: [
                                {
                                  key: 'ai',
                                  icon: <RobotOutlined />,
                                  label: t('assistTask.aiAnalyze'),
                                  onClick: handleAssistAi,
                                },
                                {
                                  key: 'manual',
                                  icon: <UserOutlined />,
                                  label: t('assistTask.manualTicket'),
                                  onClick: handleAssistManual,
                                },
                              ],
                            }}
                            disabled={!inputValue}
                          >
                            <Button
                              icon={<RobotOutlined />}
                              disabled={!inputValue}
                            >
                              <Space size={4}>
                                {t('assistTask.aiAssist')}
                                <DownOutlined style={{ fontSize: 10 }} />
                              </Space>
                            </Button>
                          </Dropdown>
                        </div>
                      </div>
                    <CodeEditor
                      key={`wpl-${workspaceMode}-${activeWplInstance?.id || activeWplIndex}`}
                      className="code-area code-area--large"
                      language="wpl"
                      value={ruleValue}
                      onChange={value => setRuleValue(value)}
                    />
                    </div>
                  </div>

                  <div className="split-col">
                    <div className="panel-block panel-block--stretch panel-block--scrollable">
                      <div className="block-header">
                        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                          <h3>{t('simulateDebug.parseResult.title')}</h3>
                          <div className="mode-toggle">
                            <button
                              type="button"
                              className={`toggle-btn ${viewMode === 'table' ? 'is-active' : ''}`}
                              onClick={() => setViewMode('table')}
                            >
                              {t('simulateDebug.parseResult.tableMode')}
                            </button>
                            <button
                              type="button"
                              className={`toggle-btn ${viewMode === 'json' ? 'is-active' : ''}`}
                              onClick={() => setViewMode('json')}
                            >
                              {t('simulateDebug.parseResult.jsonMode')}
                            </button>
                          </div>
                        </div>
                        <label className="switch">
                          <input
                            type="checkbox"
                            checked={showEmpty}
                            onChange={e => setShowEmpty(e.target.checked)}
                          />
                          <span className="switch-slider"></span>
                          <span className="switch-label">
                            {t('simulateDebug.parseResult.showEmpty')}
                          </span>
                        </label>
                      </div>
                      <div className={`mode-content ${viewMode === 'table' ? 'is-active' : ''}`}>
                        {parseError ? (
                          renderParseError()
                        ) : result ? (
                          <div style={{ paddingBottom: '10px' }}>
                            <Table
                              size="small"
                              columns={resultColumns}
                              dataSource={filterFieldsByShowEmpty(
                                processFieldsForDisplay(result.fields, result.formatJson),
                                showEmpty
                              )}
                              pagination={false}
                              rowKey="no"
                              className="data-table compact"
                              scroll={{ y: 'calc(100vh - 450px)', scrollToFirstRowOnChange: true }}
                            />
                          </div>
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            {t('simulateDebug.parseResult.clickToParse')}
                          </div>
                        )}
                      </div>
                      <div className={`mode-content ${viewMode === 'json' ? 'is-active' : ''}`}>
                        {parseError ? (
                          renderParseError()
                        ) : result ? (
                          <SyntaxHighlighter
                            className="code-block"
                            language="json"
                            style={oneDark}
                            customStyle={{ 
                              margin: 0, 
                              background: '#0f172a',
                              maxWidth: '100%',
                              width: '100%',
                              overflowX: 'hidden'
                            }}
                            codeTagProps={{ style: { background: 'transparent' } }}
                            wrapLines
                            lineProps={{ style: { background: 'transparent' } }}
                            wrapLongLines
                          >
                            {formatJsonForDisplay(result.formatJson, {
                              ...result,
                              fields: filterFieldsByShowEmpty(
                                processFieldsForDisplay(result.fields, result.formatJson),
                                showEmpty
                              ),
                            })}
                          </SyntaxHighlighter>
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            {t('simulateDebug.parseResult.clickToParse')}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              </>
            )}

            {/* 转换页面 */}
            {activeKey === 'convert' && (
              <div className="split-layout transform-layout">
                <div className="split-col transform-col">
                  <div className="panel-block panel-block--stretch panel-block--fill">
                    <div className="block-header" style={{ flexWrap: 'nowrap', alignItems: 'center' }}>
                      <div style={{ minWidth: 0, flex: 1, display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                        <h3>{t('simulateDebug.omlInput.title')}</h3>
                        {workspaceMode === 'workspace' && (
                          <InstanceSelector
                            instances={omlInstances}
                            activeIndex={activeOmlIndex}
                            maxInstances={10}
                            onSwitch={switchOmlInstance}
                            onAdd={addOmlInstance}
                            onRemove={removeOmlInstance}
                            onRename={renameOmlInstance}
                            inline
                            inlineMaxWidth="400px"
                            showAddButton={false}
                            collapseThreshold={6}
                          />
                        )}
                      </div>
                      <div className="block-actions" style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'nowrap', minWidth: 0 }}>
                        {workspaceMode === 'workspace' && (
                          <button
                            type="button"
                            className="btn primary"
                            onClick={addOmlInstance}
                            disabled={omlInstances.length >= 10}
                            title={omlInstances.length >= 10
                              ? t('multipleInstances.maxInstancesReached')
                              : t('multipleInstances.addInstance')}
                          >
                            {t('multipleInstances.addInstance')}
                          </button>
                        )}
                        <button type="button" className="btn primary" onClick={omlFormat}>
                          {t('simulateDebug.omlInput.format')}
                        </button>
                        <button
                          type="button"
                          className="btn primary"
                          onClick={handleTransform}
                          disabled={loading}
                        >
                          {loading
                            ? t('simulateDebug.omlInput.converting')
                            : t('simulateDebug.omlInput.convert')}
                        </button>
                        <button
                          type="button"
                          className="btn ghost"
                          onClick={() => setTransformOml('')}
                        >
                          {t('simulateDebug.omlInput.clear')}
                        </button>
                      </div>
                    </div>
                    <CodeEditor
                      key={`oml-${workspaceMode}-${activeOmlInstance?.id || activeOmlIndex}`}
                      className="code-area code-area--large"
                      language="oml"
                      value={transformOml}
                      onChange={value => setTransformOml(value)}
                    />
                  </div>
                </div>
                <div className="split-col transform-col">
                  <div className="panel-block panel-block--scrollable">
                    <div className="block-header">
                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                        <h3>{t('simulateDebug.parseResult.title')}</h3>
                        <div className="mode-toggle">
                          <button
                            type="button"
                            className={`toggle-btn ${
                              transformParseViewMode === 'table' ? 'is-active' : ''
                            }`}
                            onClick={() => setTransformParseViewMode('table')}
                          >
                            {t('simulateDebug.parseResult.tableMode')}
                          </button>
                          <button
                            type="button"
                            className={`toggle-btn ${
                              transformParseViewMode === 'json' ? 'is-active' : ''
                            }`}
                            onClick={() => setTransformParseViewMode('json')}
                          >
                            {t('simulateDebug.parseResult.jsonMode')}
                          </button>
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={transformParseShowEmpty}
                          onChange={e => setTransformParseShowEmpty(e.target.checked)}
                        />
                        <span className="switch-slider"></span>
                        <span className="switch-label">
                          {t('simulateDebug.parseResult.showEmpty')}
                        </span>
                      </label>
                    </div>
                    <div
                      className={`mode-content ${
                        transformParseViewMode === 'table' ? 'is-active' : ''
                      }`}
                    >
                      {transformParseResult ? (
                        <div style={{ paddingBottom: '10px' }}>
                          <Table
                            size="small"
                            columns={resultColumns}
                            dataSource={filterFieldsByShowEmpty(
                              processFieldsForDisplay(transformParseResult.fields, transformParseResult.formatJson),
                              transformParseShowEmpty
                            )}
                            pagination={false}
                            rowKey="no"
                            className="data-table compact"
                            scroll={{ y: 'calc(50vh - 300px)', scrollToFirstRowOnChange: true }}
                          />
                        </div>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          {t('simulateDebug.parseResult.willShowHere')}
                        </div>
                      )}
                    </div>
                    <div
                      className={`mode-content ${
                        transformParseViewMode === 'json' ? 'is-active' : ''
                      }`}
                    >
                      {transformParseResult ? (
                        <SyntaxHighlighter
                          className="code-block"
                          language="json"
                          style={oneDark}
                          customStyle={{ 
                            margin: 0, 
                            background: '#0f172a',
                            maxWidth: '100%',
                            width: '100%',
                            overflowX: 'hidden'
                          }}
                          codeTagProps={{ style: { background: 'transparent' } }}
                          wrapLines
                          lineProps={{ style: { background: 'transparent' } }}
                          wrapLongLines
                        >
                          {formatJsonForDisplay(transformParseResult.formatJson, {
                            ...transformParseResult,
                            fields: filterFieldsByShowEmpty(
                              processFieldsForDisplay(transformParseResult.fields, transformParseResult.formatJson),
                              transformParseShowEmpty
                            ),
                          })}
                        </SyntaxHighlighter>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          {t('simulateDebug.parseResult.willShowHere')}
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="panel-block panel-block--scrollable">
                    <div className="block-header">
                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                        <h3>{t('simulateDebug.convertResult.title')}</h3>
                        <div className="mode-toggle">
                          <button
                            type="button"
                            className={`toggle-btn ${
                              transformResultViewMode === 'table' ? 'is-active' : ''
                            }`}
                            onClick={() => setTransformResultViewMode('table')}
                          >
                            {t('simulateDebug.parseResult.tableMode')}
                          </button>
                          <button
                            type="button"
                            className={`toggle-btn ${
                              transformResultViewMode === 'json' ? 'is-active' : ''
                            }`}
                            onClick={() => setTransformResultViewMode('json')}
                          >
                            {t('simulateDebug.parseResult.jsonMode')}
                          </button>
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={transformResultShowEmpty}
                          onChange={e => setTransformResultShowEmpty(e.target.checked)}
                        />
                        <span className="switch-slider"></span>
                        <span className="switch-label">
                          {t('simulateDebug.parseResult.showEmpty')}
                        </span>
                      </label>
                    </div>
                    <div
                      className={`mode-content ${
                        transformResultViewMode === 'table' ? 'is-active' : ''
                      }`}
                    >
                      {transformError ? (
                        renderTransformError()
                      ) : transformResult ? (
                        <div style={{ paddingBottom: '10px' }}>
                          <Table
                            size="small"
                            columns={resultColumns}
                            dataSource={filterFieldsByShowEmpty(
                              transformResult.fields,
                              transformResultShowEmpty
                            )}
                            pagination={false}
                            rowKey="no"
                            className="data-table compact"
                            scroll={{ y: 'calc(50vh - 300px)', scrollToFirstRowOnChange: true }}
                          />
                        </div>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          {t('simulateDebug.convertResult.willShowHere')}
                        </div>
                      )}
                    </div>
                    <div
                      className={`mode-content ${
                        transformResultViewMode === 'json' ? 'is-active' : ''
                      }`}
                    >
                      {transformError ? (
                        renderTransformError()
                      ) : transformResult ? (
                        <SyntaxHighlighter
                          className="code-block"
                          language="json"
                          style={oneDark}
                          customStyle={{ 
                            margin: 0, 
                            background: '#0f172a',
                            maxWidth: '100%',
                            width: '100%',
                            overflowX: 'hidden'
                          }}
                          codeTagProps={{ style: { background: 'transparent' } }}
                          wrapLines
                          lineProps={{ style: { background: 'transparent' } }}
                          wrapLongLines
                        >
                          {formatJsonForDisplay(
                            transformResult.formatJson,
                            {
                              ...transformResult,
                              fields: filterFieldsByShowEmpty(
                                transformResult.fields,
                                transformResultShowEmpty
                              ),
                            },
                            parsed =>
                              transformResultShowEmpty ? parsed : filterEmptyFields(parsed)
                          )}
                        </SyntaxHighlighter>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          {t('simulateDebug.convertResult.willShowHere')}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* 知识库页面 */}
            {activeKey === 'knowledge' && (
              <div className="split-layout knowledge-layout">
                <div className="split-col knowledge-col">
                    <div className="panel-block">
                      <div className="block-header">
                        <h3>{t('simulateDebug.knowledge.sqlQuery')}</h3>
                        <div className="block-actions">
                          <button type="button" className="btn ghost" onClick={handleKnowledgeUpdate}>{t('simulateDebug.knowledge.update')}</button>
                          <button
                            type="button"
                            className="btn primary"
                            onClick={handleKnowledgeQuery}
                            disabled={knowledgeLoading}
                          >
                            {knowledgeLoading ? t('simulateDebug.knowledge.querying') : t('simulateDebug.knowledge.query')}
                          </button>
                        </div>
                      </div>
                      <div className="form-grid compact compact--single">
                        <div className="form-row">
                          <label>{t('simulateDebug.knowledge.selectTable')}</label>
                          <select
                            value={knowledgeTable}
                            onChange={(e) => handleKnowledgeTableChange(e.target.value)}
                          >
                            {knowledgeDatasets.length > 0 ? (
                              knowledgeDatasets.map((dataset) => (
                                <option key={dataset} value={dataset}>
                                  {dataset}
                                </option>
                              ))
                            ) : (
                              <option value="">{t('simulateDebug.knowledge.loading')}</option>
                            )}
                          </select>
                        </div>
                      </div>
                      <CodeEditor
                        className="code-area code-area--large"
                        value={knowledgeSql}
                        onChange={(value) => setKnowledgeSql(value)}
                        language="toml"
                        theme="vscodeDark"
                      />
                    </div>
                  </div>
                  <div className="split-col knowledge-col">
                    <div className="panel-block panel-block--stretch panel-block--scrollable">
                      <div className="block-header">
                        <h3>{t('simulateDebug.knowledge.queryResult')}</h3>
                        <label className="switch">
                          <input
                            type="checkbox"
                            checked={showEmpty}
                            onChange={(e) => setShowEmpty(e.target.checked)}
                          />
                          <span className="switch-slider"></span>
                          <span className="switch-label">{t('simulateDebug.knowledge.showEmpty')}</span>
                        </label>
                      </div>
                      <div className="mode-toggle">
                        <button
                          type="button"
                          className={`toggle-btn ${knowledgeViewMode === 'table' ? 'is-active' : ''}`}
                          onClick={() => setKnowledgeViewMode('table')}
                        >
                          {t('simulateDebug.knowledge.tableMode')}
                        </button>
                        <button
                          type="button"
                          className={`toggle-btn ${knowledgeViewMode === 'json' ? 'is-active' : ''}`}
                          onClick={() => setKnowledgeViewMode('json')}
                        >
                          {t('simulateDebug.knowledge.jsonMode')}
                        </button>
                      </div>
                      <div className={`mode-content ${knowledgeViewMode === 'table' ? 'is-active' : ''}`}>
                        {knowledgeResult ? (
                          <Table
                            size="small"
                            columns={(knowledgeResult.columns || []).map((column) => ({
                              ...column,
                              width: column.width || 160,
                              ellipsis: false,
                            }))}
                            dataSource={knowledgeResult.fields || []}
                            pagination={false}
                            rowKey="key"
                            className="data-table compact"
                            scroll={{ x: 'max-content' }}
                          />
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            点击"查询"按钮查看结果
                          </div>
                        )}
                      </div>
                      <div className={`mode-content ${knowledgeViewMode === 'json' ? 'is-active' : ''}`}>
                        {knowledgeResult ? (
                          <pre className="code-block">{JSON.stringify(knowledgeResult.fields || [], null, 2)}</pre>
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            点击"查询"按钮查看结果
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* 性能测试页面 */}
              {activeKey === 'performance' && (
                <div className="split-layout performance-layout">
                  <div className="split-col performance-col performance-col--left">
                    <div className="panel-block">
                      <div className="block-header">
                        <h3>{t('simulateDebug.performance.sampleData')}</h3>
                        <div className="block-actions">
                          <button
                            type="button"
                            className="btn primary"
                            onClick={async () => {
                              setLoading(true);
                              try {
                                // 模拟性能测试
                                await new Promise((resolve) => setTimeout(resolve, 2000));
                                setPerformanceResult(`== Sinks ==
business   | /sink/benchmark/[0]                      | ././out/benchmark.dat                                        | 1000
infras     | monitor/[0]                              | ././data/out_dat/monitor.dat                                 | 0
infras     | default/[0]                              | ././data/out_dat/default.dat                                 | 0
infras     | error/[0]                                | ././data/out_dat/error.dat                                   | 0
infras     | intercept/[0]                            | ././data/out_dat/intercept.dat                               | 0
infras     | miss/[0]                                 | ././data/out_dat/miss.dat                                    | 0
infras     | residue/[0]                              | ././data/out_dat/residue.dat                                 | 0
-- total lines: 1000
validate: PASS

| Group           | Sink | Total | Actual | Ratio | Expect    | Verdict |
|-----------------|------|-------|--------|-------|-----------|---------|
| /sink/benchmark | [0]  |  1000 |  1000  |   1   |   1±0.01  |    OK   |
| monitor         | [0]  |  1000 |    0   |   0   |     -     |    -    |
| default         | [0]  |  1000 |    0   |   0   |   0±0.02  |    OK   |
| error           | [0]  |  1000 |    0   |   0   | 0.01±0.02 |    OK   |
| intercept       | [0]  |  1000 |    0   |   0   |     -     |    -    |
| miss            | [0]  |  1000 |    0   |   0   |  [0 ~ 2]  |    OK   |
| residue         | [0]  |  1000 |    0   |   0   |     -     |    -    |`);
                              } finally {
                                setLoading(false);
                              }
                            }}
                            disabled={loading}
                          >
                            {loading ? t('simulateDebug.performance.testing') : t('simulateDebug.performance.test')}
                          </button>
                        </div>
                      </div>
                      <CodeEditor
                        className="code-area"
                        value={performanceSample}
                        onChange={(value) => setPerformanceSample(value)}
                        language="toml"
                        theme="vscodeDark"
                      />
                    </div>
                    <div className="panel-block panel-block--stretch">
                      <div className="block-header">
                        <h3>{t('simulateDebug.performance.dataGenConfig')}</h3>
                      </div>
                      <CodeEditor
                        className="code-area code-area--large"
                        value={performanceConfig}
                        onChange={(value) => setPerformanceConfig(value)}
                        language="toml"
                        theme="vscodeDark"
                      />
                    </div>
                  </div>
                  <div className="split-col performance-col performance-col--right">
                    <div className="panel-block panel-block--stretch">
                      <div className="block-header">
                        <h3>{t('simulateDebug.performance.executionResult')}</h3>
                        <p className="block-desc">{t('simulateDebug.performance.outputDesc')}</p>
                      </div>
                      {performanceResult ? (
                        <pre className="code-block code-block--scroll">{performanceResult}</pre>
                      ) : (
                        <pre className="code-block code-block--scroll" style={{ color: '#999' }}>
                          {t('simulateDebug.performance.clickToTest')}
                        </pre>
                      )}
                    </div>
                  </div>
                </div>
              )}

            <Modal
              title="选择解析规则（WPL）"
              open={wplModalVisible}
              onOk={handleConfirmLoadWpl}
              onCancel={() => setWplModalVisible(false)}
              okText="加载到编辑器"
              cancelText="取消"
            >
              <div
                style={{
                  padding: '8px 4px 0',
                  marginBottom: 12,
                  fontSize: 13,
                  color: '#666',
                }}
              >
                从规则仓库中选择一条解析规则，加载后可在下方编辑器中继续修改并保存。
              </div>
              <div
                style={{
                  border: '1px solid #f0f0f0',
                  borderRadius: 6,
                  padding: 8,
                  background: '#fafafa',
                }}
              >
                <Select
                  style={{ width: '100%' }}
                  value={selectedWplFile || undefined}
                  onChange={(value) => setSelectedWplFile(value)}
                  placeholder={
                    wplFiles.length === 0 ? '暂无可用规则' : '请选择解析规则文件'
                  }
                  options={wplFiles.map((fileName) => ({
                    label: fileName,
                    value: fileName,
                  }))}
                  showSearch
                  optionFilterProp="label"
                />
              </div>
            </Modal>

            <Modal
              title="选择 OML 规则"
              open={omlModalVisible}
              onOk={handleConfirmLoadOml}
              onCancel={() => setOmlModalVisible(false)}
              okText="加载到编辑器"
              cancelText="取消"
            >
              <div
                style={{
                  padding: '8px 4px 0',
                  marginBottom: 12,
                  fontSize: 13,
                  color: '#666',
                }}
              >
                从规则仓库中选择一条 OML 转换规则，加载后可在左侧编辑器中继续修改并保存。
              </div>
              <div
                style={{
                  border: '1px solid #f0f0f0',
                  borderRadius: 6,
                  padding: 8,
                  background: '#fafafa',
                }}
              >
                <Select
                  style={{ width: '100%' }}
                  value={selectedOmlFile || undefined}
                  onChange={(value) => setSelectedOmlFile(value)}
                  placeholder={
                    omlFiles.length === 0 ? '暂无可用规则' : '请选择 OML 规则文件'
                  }
                  options={omlFiles.map((fileName) => ({
                    label: fileName,
                    value: fileName,
                  }))}
                  showSearch
                  optionFilterProp="label"
                />
              </div>
            </Modal>
          </section>
        </article>
      </section>

      {/* AI 辅助结果抽屉：从底部滑出，展示规则建议，支持一键填入全部区域并执行解析和转换 */}
      <AssistResultDrawer
        open={assistDrawerOpen}
        task={assistDrawerTask}
        onFillAll={handleFillAll}
        onClose={() => setAssistDrawerOpen(false)}
      />

      {/* 人工提单弹窗：填写补充说明后提交工单 */}
      <ManualTicketModal
        open={manualModalOpen}
        logData={inputValue}
        currentWpl={ruleValue}
        currentOml={transformOml}
        defaultTargetRule="both"
        onSubmit={handleManualSubmit}
        onClose={() => setManualModalOpen(false)}
      />
    </>
  );
}

export default SimulateDebugPage;
