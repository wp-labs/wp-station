import React, { useState, useEffect } from 'react';
import { Table, message, Modal, Select } from 'antd';
import { parseLogs, convertRecord } from '@/services/debug';
import { RuleType, fetchRuleFiles, fetchRuleConfig, saveRuleConfig, executeKnowledgeSql} from '@/services/config';

/**
 * 模拟调试页面
 * 功能：
 * 1. 日志解析调试
 * 2. 记录转换调试
 * 3. 知识库状态查询
 * 4. 性能测试
 * 对应原型：pages/views/simulate-debug/simulate-parse.html
 */

// 示例日志数据
const EXAMPLE_LOG = `222.133.52.20 - - [06/Aug/2019:12:12:19 +0800] "GET /nginx-logo.png HTTP/1.1" 200 368 "http://119.122.1.4/" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/75.0.3770.142 Safari/537.36" "-"`;

// 示例解析规则
const EXAMPLE_RULE = `package /example/simple {
  rule nginx {
        (ip:sip,_^2,time:recv_time<[,]>,http/request",http/status,digit,chars",http/agent",_")
  }
}`;

function SimulateDebugPage() {
  const [activeKey, setActiveKey] = useState('parse');
  const [inputValue, setInputValue] = useState('');
  const [ruleValue, setRuleValue] = useState('');
  const [result, setResult] = useState(null);
  const [loading, setLoading] = useState(false);
  const [viewMode, setViewMode] = useState('table');
  // 解析页“显示空值”开关
  const [showEmpty, setShowEmpty] = useState(true);
  const [wplModalVisible, setWplModalVisible] = useState(false);
  const [wplFiles, setWplFiles] = useState([]);
  const [selectedWplFile, setSelectedWplFile] = useState('');
  const [currentWplFile, setCurrentWplFile] = useState('');
  const [omlModalVisible, setOmlModalVisible] = useState(false);
  const [omlFiles, setOmlFiles] = useState([]);
  const [selectedOmlFile, setSelectedOmlFile] = useState('');
  const [currentOmlFile, setCurrentOmlFile] = useState('');
  
  // 转换调试相关状态
  const [transformOml, setTransformOml] = useState('');
  const [transformParseResult, setTransformParseResult] = useState(null);
  const [transformResult, setTransformResult] = useState(null);
  const [transformParseViewMode, setTransformParseViewMode] = useState('table');
  const [transformResultViewMode, setTransformResultViewMode] = useState('table');
  // 转换页各自的“显示空值”开关（解析结果 / 转换结果互相独立）
  const [transformParseShowEmpty, setTransformParseShowEmpty] = useState(true);
  const [transformResultShowEmpty, setTransformResultShowEmpty] = useState(true);
  
  // 知识库相关状态
  const [knowledgeDatasets, setKnowledgeDatasets] = useState([]);
  const [knowledgeTable, setKnowledgeTable] = useState('');
  const [knowledgeSql, setKnowledgeSql] = useState('');
  const [knowledgeResult, setKnowledgeResult] = useState(null);
  const [knowledgeViewMode, setKnowledgeViewMode] = useState('table');
  const [knowledgeLoading, setKnowledgeLoading] = useState(false);
  
  // 性能测试相关状态
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

  /**
   * 处理测试/解析按钮点击
   * 调用服务层解析日志
   */
  const handleTest = async () => {
    setLoading(true);
    try {
      // 调用服务层解析日志（使用对象参数）
      const response = await parseLogs({ 
        logs: inputValue, 
        rules: ruleValue 
      });
      setResult(response);
      // 同步更新转换页的解析结果
      if (response && Array.isArray(response.fields)) {
        setTransformParseResult({ fields: response.fields });
      }
    } finally {
      setLoading(false);
    }
  };

  /**
   * 一键示例
   * 填充示例日志和规则,并自动执行解析,同时填充转换页面数据
   */
  const handleExample = async () => {
    setInputValue(EXAMPLE_LOG);
    setRuleValue(EXAMPLE_RULE);
    // 自动执行解析
    setLoading(true);
    try {
      const response = await parseLogs({
        logs: EXAMPLE_LOG,
        rules: EXAMPLE_RULE
      });
      setResult(response);

      // 同时填充转换页面的数据
      const exampleOml = `name : /oml/example/simple

rule :
    /example/simple*
---
recv_time  = take() ;
occur_time = Time::now() ;
from_ip    = take(option:[from-ip]) ;
src_ip     = take(option:[src-ip,sip,source-ip] );
*  = take() ;`;

      setTransformOml(exampleOml);

      // 使用真实解析结果同步到转换页面
      if (response && Array.isArray(response.fields)) {
        setTransformParseResult({ fields: response.fields });
      }
    } finally {
      setLoading(false);
    }
  };

  /**
   * 一键清空
   * 清空解析和转换的所有输入和结果
   */
  const handleClear = () => {
    // 清空解析页面
    setInputValue('');
    setRuleValue('');
    setResult(null);
    // 清空转换页面
    setTransformOml('');
    setTransformParseResult(null);
    setTransformResult(null);
  };

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

  const handleSaveWplRule = async () => {
    if (!currentWplFile) {
      message.warning('请先通过“加载规则”选择一个 WPL 文件');
      return;
    }
    try {
      await saveRuleConfig({
        type: RuleType.WPL,
        file: currentWplFile,
        content: ruleValue || '',
      });

      Modal.info({
        icon: null,
        okText: '确定',
        width: 420,
        title: '保存成功',
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
              <div
                style={{
                  fontSize: 16,
                  fontWeight: 600,
                  color: '#52c41a',
                  marginBottom: 4,
                }}
              >
                保存成功
              </div>
              <div style={{ fontSize: 13, color: '#666' }}>解析规则已成功保存。</div>
            </div>
          </div>
        ),
      });
    } catch (error) {
      message.error(`保存规则失败：${error?.message || error}`);
    }
  };

  const handleTransform = async () => {
    if (!transformOml) {
      message.warning('请先填写 OML 转换规则');
      return;
    }
    setLoading(true);
    try {
      const response = await convertRecord({ oml: transformOml });
      // 新 API 直接返回 { fields: [...] } 格式
      setTransformResult({
        fields: Array.isArray(response?.fields) ? response.fields : [],
        formatJson: response.formatJson || '',
      });
    } catch (error) {
      message.error(`执行转换失败：${error?.message || error}`);
    } finally {
      setLoading(false);
    }
  };

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

  const handleSaveOmlRule = async () => {
    if (!currentOmlFile) {
      message.warning('请先通过“加载规则”选择一个 OML 文件');
      return;
    }
    try {
      await saveRuleConfig({
        type: RuleType.OML,
        file: currentOmlFile,
        content: transformOml || '',
      });

      Modal.info({
        icon: null,
        okText: '确定',
        width: 420,
        title: '保存成功',
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
              <div
                style={{
                  fontSize: 16,
                  fontWeight: 600,
                  color: '#52c41a',
                  marginBottom: 4,
                }}
              >
                保存成功
              </div>
              <div style={{ fontSize: 13, color: '#666' }}>转换规则已成功保存。</div>
            </div>
          </div>
        ),
      });
    } catch (error) {
      message.error(`保存规则失败：${error?.message || error}`);
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
          setKnowledgeSql(`select * from ${firstDataset};`);
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
    setKnowledgeSql(`select * from ${tableName};`);
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

  const menuItems = [
    { key: 'parse', label: '解析' },
    { key: 'convert', label: '转换' },
    { key: 'knowledge', label: '知识库' },
    { key: 'performance', label: '性能测试' },
  ];

  const resultColumns = [
    { title: 'no', dataIndex: 'no', key: 'no', width: 60 },
    { title: 'meta', dataIndex: 'meta', key: 'meta', width: 120 },
    { title: 'name', dataIndex: 'name', key: 'name', width: 150 },
    { title: 'value', dataIndex: 'value', key: 'value', ellipsis: true },
  ];

  /**
   * 按“显示空值”开关过滤字段列表
   * showEmptyFlag=false 时，过滤掉 value 为空字符串/null/undefined 的行
   */
  const filterFieldsByShowEmpty = (fields, showEmptyFlag) => {
    const list = Array.isArray(fields) ? fields : [];
    if (showEmptyFlag) {
      return list;
    }
    return list.filter((fieldItem) => {
      const fieldValue = fieldItem && fieldItem.value;
      return fieldValue !== '' && fieldValue !== null && fieldValue !== undefined;
    });
  };

  // 获取页面标题（与旧版本一致）
  const getPageTitle = () => {
    const titles = {
      parse: '解析调试',
      convert: '转换调试',
      knowledge: '知识库',
      performance: '性能测试',
    };
    return titles[activeKey] || '模拟调试';
  };

  return (
    <>
      <aside className="side-nav" data-group="simulate-debug">
        <h2>模拟调试</h2>
        <button
          type="button"
          className={`side-item ${activeKey === 'parse' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('parse')}
        >
          解析
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'convert' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('convert')}
        >
          转换
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'knowledge' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('knowledge')}
        >
          知识库
        </button>
        <button
          type="button"
          className={`side-item ${activeKey === 'performance' ? 'is-active' : ''}`}
          onClick={() => setActiveKey('performance')}
        >
          性能测试
        </button>
      </aside>

      <section className="page-panels">
        <article className="panel is-visible">
          <header className="panel-header">
            <h2>{getPageTitle()}</h2>
          </header>
          <section className="panel-body simulate-body">
            {/* 解析调试页面 */}
            {activeKey === 'parse' && (
              <>
                <div className="panel-block">
                  <div className="block-header">
                    <div>
                      <h3>日志数据</h3>
                      <p className="block-desc">粘贴实时采集的原始日志，支持文本与文件导入。</p>
                    </div>
                    <div className="block-actions">
                      <button type="button" className="btn tertiary" onClick={handleExample}>
                        一键示例
                      </button>
                      <button type="button" className="btn ghost" onClick={handleClear}>
                        一键清空
                      </button>
                    </div>
                  </div>
                  <textarea
                    className="code-area"
                    rows={7}
                    value={inputValue}
                    onChange={(e) => setInputValue(e.target.value)}
                    placeholder="粘贴实时采集的原始日志..."
                  />
                </div>

                <div className="split-layout">
                  <div className="split-col">
                    <div className="panel-block panel-block--stretch">
                      <div className="block-header">
                        <h3>解析规则</h3>
                        <div className="block-actions">
                          <button
                            type="button"
                            className="btn ghost"
                            onClick={handleOpenWplModal}
                          >
                            加载规则
                          </button>
                          <button
                            type="button"
                            className="btn tertiary"
                            onClick={handleSaveWplRule}
                          >
                            保存规则
                          </button>
                          <button
                            type="button"
                            className="btn primary"
                            onClick={handleTest}
                            disabled={loading}
                          >
                            {loading ? '解析中...' : '解析'}
                          </button>
                        </div>
                      </div>
                      <textarea
                        className="code-area code-area--large"
                        rows={12}
                        value={ruleValue}
                        onChange={(e) => setRuleValue(e.target.value)}
                        placeholder="输入解析规则..."
                      />
                    </div>
                  </div>

                  <div className="split-col">
                    <div className="panel-block panel-block--stretch panel-block--scrollable">
                      <div className="block-header">
                        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                          <h3>解析结果</h3>
                          <div className="mode-toggle">
                            <button
                              type="button"
                              className={`toggle-btn ${viewMode === 'table' ? 'is-active' : ''}`}
                              onClick={() => setViewMode('table')}
                            >
                              表格模式
                            </button>
                            <button
                              type="button"
                              className={`toggle-btn ${viewMode === 'json' ? 'is-active' : ''}`}
                              onClick={() => setViewMode('json')}
                            >
                              JSON 模式
                            </button>
                          </div>
                        </div>
                        <label className="switch">
                          <input
                            type="checkbox"
                            checked={showEmpty}
                            onChange={(e) => setShowEmpty(e.target.checked)}
                          />
                          <span className="switch-slider"></span>
                          <span className="switch-label">显示空值</span>
                        </label>
                      </div>
                      <div className={`mode-content ${viewMode === 'table' ? 'is-active' : ''}`}>
                        {result ? (
                          <Table
                            size="small"
                            columns={resultColumns}
                            dataSource={filterFieldsByShowEmpty(result.fields, showEmpty)}
                            pagination={false}
                            rowKey="no"
                            className="data-table compact"
                          />
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            点击"解析"按钮查看结果
                          </div>
                        )}
                      </div>
                      <div className={`mode-content ${viewMode === 'json' ? 'is-active' : ''}`}>
                        {result ? (
                          <pre className="code-block">
                            {JSON.stringify(
                              {
                                ...result,
                                fields: filterFieldsByShowEmpty(result.fields, showEmpty),
                              },
                              null,
                              2,
                            )}
                          </pre>
                        ) : (
                          <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                            点击"解析"按钮查看结果
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              </>
            )}

            {/* 转换调试页面 */}
            {activeKey === 'convert' && (
              <div className="split-layout transform-layout">
                <div className="split-col transform-col">
                  <div className="panel-block panel-block--stretch panel-block--fill">
                    <div className="block-header">
                      <div>
                        <h3>OML 输入</h3>
                        <p className="block-desc">根据解析结果补齐转换逻辑，支持断点调试。</p>
                      </div>
                      <div className="block-actions">
                        <button
                          type="button"
                          className="btn ghost"
                          onClick={handleOpenOmlModal}
                        >
                          加载规则
                        </button>
                        <button
                          type="button"
                          className="btn tertiary"
                          onClick={handleSaveOmlRule}
                        >
                          保存规则
                        </button>
                        <button
                          type="button"
                          className="btn primary"
                          onClick={handleTransform}
                          disabled={loading}
                        >
                          {loading ? '转换中...' : '转换'}
                        </button>
                        <button
                          type="button"
                          className="btn ghost"
                          onClick={() => setTransformOml('')}
                        >
                          清空
                        </button>
                      </div>
                    </div>
                    <textarea
                      className="code-area code-area--large"
                      rows={14}
                      value={transformOml}
                      onChange={(e) => setTransformOml(e.target.value)}
                      placeholder="输入 OML 转换规则..."
                      spellCheck={false}
                    />
                  </div>
                </div>
                <div className="split-col transform-col">
                  <div className="panel-block panel-block--scrollable">
                    <div className="block-header">
                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                        <h3>解析结果</h3>
                        <div className="mode-toggle">
                          <button
                            type="button"
                            className={`toggle-btn ${transformParseViewMode === 'table' ? 'is-active' : ''}`}
                            onClick={() => setTransformParseViewMode('table')}
                          >
                            表格模式
                          </button>
                          <button
                            type="button"
                            className={`toggle-btn ${transformParseViewMode === 'json' ? 'is-active' : ''}`}
                            onClick={() => setTransformParseViewMode('json')}
                          >
                            JSON 模式
                          </button>
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={transformParseShowEmpty}
                          onChange={(e) => setTransformParseShowEmpty(e.target.checked)}
                        />
                        <span className="switch-slider"></span>
                        <span className="switch-label">显示空值</span>
                      </label>
                    </div>
                    <div className={`mode-content ${transformParseViewMode === 'table' ? 'is-active' : ''}`}>
                      {transformParseResult ? (
                        <Table
                          size="small"
                          columns={resultColumns}
                          dataSource={filterFieldsByShowEmpty(
                            transformParseResult.fields,
                            transformParseShowEmpty,
                          )}
                          pagination={false}
                          rowKey="no"
                          className="data-table compact"
                        />
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          解析结果将显示在这里
                        </div>
                      )}
                    </div>
                    <div className={`mode-content ${transformParseViewMode === 'json' ? 'is-active' : ''}`}>
                      {transformParseResult ? (
                        <pre className="code-block">
                          {JSON.stringify(
                            {
                              ...transformParseResult,
                              fields: filterFieldsByShowEmpty(
                                transformParseResult.fields,
                                transformParseShowEmpty,
                              ),
                            },
                            null,
                            2,
                          )}
                        </pre>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          解析结果将显示在这里
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="panel-block panel-block--scrollable">
                    <div className="block-header">
                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                        <h3>转换结果</h3>
                        <div className="mode-toggle">
                          <button
                            type="button"
                            className={`toggle-btn ${transformResultViewMode === 'table' ? 'is-active' : ''}`}
                            onClick={() => setTransformResultViewMode('table')}
                          >
                            表格模式
                          </button>
                          <button
                            type="button"
                            className={`toggle-btn ${transformResultViewMode === 'json' ? 'is-active' : ''}`}
                            onClick={() => setTransformResultViewMode('json')}
                          >
                            JSON 模式
                          </button>
                        </div>
                      </div>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={transformResultShowEmpty}
                          onChange={(e) => setTransformResultShowEmpty(e.target.checked)}
                        />
                        <span className="switch-slider"></span>
                        <span className="switch-label">显示空值</span>
                      </label>
                    </div>
                    <div className={`mode-content ${transformResultViewMode === 'table' ? 'is-active' : ''}`}>
                      {transformResult ? (
                        <Table
                          size="small"
                          columns={resultColumns}
                          dataSource={filterFieldsByShowEmpty(
                            transformResult.fields,
                            transformResultShowEmpty,
                          )}
                          pagination={false}
                          rowKey="no"
                          className="data-table compact"
                        />
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          转换结果将显示在这里
                        </div>
                      )}
                    </div>
                    <div className={`mode-content ${transformResultViewMode === 'json' ? 'is-active' : ''}`}>
                      {transformResult ? (
                        <pre className="code-block">
                          {(() => {
                            if (transformResult.formatJson) {
                              try {
                                const parsed = JSON.parse(transformResult.formatJson);
                                return JSON.stringify(parsed, null, 2);
                              } catch (_e) {
                                // 如果不是严格 JSON 字符串，则按原样输出
                                return transformResult.formatJson;
                              }
                            }
                            return JSON.stringify(
                              {
                                ...transformResult,
                                fields: filterFieldsByShowEmpty(
                                  transformResult.fields,
                                  transformResultShowEmpty,
                                ),
                              },
                              null,
                              2,
                            );
                          })()}
                        </pre>
                      ) : (
                        <div style={{ padding: '40px', textAlign: 'center', color: '#999' }}>
                          转换结果将显示在这里
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
                  <div className="panel-block panel-block--stretch">
                    <div className="block-header">
                      <h3>SQL 查询</h3>
                      <div className="block-actions">
                        <button type="button" className="btn ghost" onClick={handleKnowledgeUpdate}>更新</button>
                        <button
                          type="button"
                          className="btn primary"
                          onClick={handleKnowledgeQuery}
                          disabled={knowledgeLoading}
                        >
                          {knowledgeLoading ? '查询中...' : '查询'}
                        </button>
                      </div>
                    </div>
                    <div className="form-grid compact compact--single">
                      <div className="form-row">
                        <label>选择知识库表</label>
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
                            <option value="">加载中...</option>
                          )}
                        </select>
                      </div>
                    </div>
                    <textarea
                      className="code-area code-area--large"
                      rows={12}
                      value={knowledgeSql}
                      onChange={(e) => setKnowledgeSql(e.target.value)}
                      spellCheck={false}
                    />
                  </div>
                </div>
                <div className="split-col knowledge-col">
                  <div className="panel-block panel-block--stretch panel-block--scrollable">
                    <div className="block-header">
                      <h3>查询结果</h3>
                      <label className="switch">
                        <input
                          type="checkbox"
                          checked={showEmpty}
                          onChange={(e) => setShowEmpty(e.target.checked)}
                        />
                        <span className="switch-slider"></span>
                        <span className="switch-label">显示空值</span>
                      </label>
                    </div>
                    <div className="mode-toggle">
                      <button
                        type="button"
                        className={`toggle-btn ${knowledgeViewMode === 'table' ? 'is-active' : ''}`}
                        onClick={() => setKnowledgeViewMode('table')}
                      >
                        表格模式
                      </button>
                      <button
                        type="button"
                        className={`toggle-btn ${knowledgeViewMode === 'json' ? 'is-active' : ''}`}
                        onClick={() => setKnowledgeViewMode('json')}
                      >
                        JSON 模式
                      </button>
                    </div>
                    <div className={`mode-content ${knowledgeViewMode === 'table' ? 'is-active' : ''}`}>
                      {knowledgeResult ? (
                        <Table
                          size="small"
                          columns={knowledgeResult.columns || []}
                          dataSource={knowledgeResult.fields || []}
                          pagination={false}
                          rowKey="key"
                          className="data-table compact"
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
                      <h3>样本数据</h3>
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
                          {loading ? '测试中...' : '测试'}
                        </button>
                      </div>
                    </div>
                    <textarea
                      className="code-area"
                      rows={6}
                      value={performanceSample}
                      onChange={(e) => setPerformanceSample(e.target.value)}
                      spellCheck={false}
                    />
                  </div>
                  <div className="panel-block panel-block--stretch">
                    <div className="block-header">
                      <h3>数据生成配置（TOML）</h3>
                    </div>
                    <textarea
                      className="code-area code-area--large"
                      rows={14}
                      value={performanceConfig}
                      onChange={(e) => setPerformanceConfig(e.target.value)}
                      spellCheck={false}
                    />
                  </div>
                </div>
                <div className="split-col performance-col performance-col--right">
                  <div className="panel-block panel-block--stretch">
                    <div className="block-header">
                      <h3>执行结果</h3>
                      <p className="block-desc">脚本输出支持导出与分享。</p>
                    </div>
                    {performanceResult ? (
                      <pre className="code-block code-block--scroll">{performanceResult}</pre>
                    ) : (
                      <pre className="code-block code-block--scroll" style={{ color: '#999' }}>
                        点击"测试"按钮查看执行结果
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
    </>
  );
}

export default SimulateDebugPage;
