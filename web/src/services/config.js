/**
 * 配置管理服务模块
 * 提供规则配置的查询、校验和保存功能
 * 部分类型已接入真实 API，剩余类型仍使用 Mock 数据
 */

import httpRequest from './request';
import { getSharedSystem, resolveSystem } from './system';

// 前端枚举：规则 / 连接类型，与后端 RuleType 枚举保持一致
export const RuleType = Object.freeze({
  SOURCE: 'source',
  SINK: 'sink',
  PARSE: 'parse',
  SOURCE_CONNECT: 'source_connect',
  SINK_CONNECT: 'sink_connect',
  WPL: 'wpl',
  OML: 'oml',
  WINDOWS: 'windows',
  SCHEMA: 'schema',
  RULE: 'rule',
  SCENARIOS: 'scenarios',
  KNOWLEDGE: 'knowledge',
});

const KNOWLEDGE_SAVE_TIMEOUT_MS = 60_000;

const uniqueNames = (items) => Array.from(new Set((items || []).filter(Boolean)));
const uniqueConnectionItems = (items) => {
  const seen = new Set();

  return (items || []).filter((item) => {
    const file = item?.file;
    if (!file || seen.has(file)) {
      return false;
    }

    seen.add(file);
    return true;
  });
};

const getConnectionDisplayName = (file, displayName) => {
  if (displayName && String(displayName).trim()) {
    return String(displayName).trim();
  }

  return file?.replace(/\.toml$/i, '') || '';
};

const configFileMetaCache = new Map();

const normalizeConfigFileItems = (items = []) =>
  Array.isArray(items)
    ? items
        .map((item) => {
          const file = typeof item?.file === 'string' ? item.file : '';
          if (!file) {
            return null;
          }

          return {
            file,
            displayName:
              typeof item?.display_name === 'string' && item.display_name.trim()
                ? item.display_name.trim()
                : undefined,
            sortOrder:
              typeof item?.sort_order === 'number' && item.sort_order >= 0
                ? item.sort_order
                : undefined,
          };
        })
        .filter(Boolean)
    : [];

async function fetchConfigFileListResponse(ruleType, keyword, system) {
  const targetSystem = resolveSystem(system);
  const keywordParam =
    typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;
  const response = await httpRequest.get('/config/files', {
    params: {
      system: targetSystem,
      rule_type: ruleType,
      keyword: keywordParam,
    },
  });

  const items = normalizeConfigFileItems(response?.items);
  const defaultFile =
    typeof response?.default_file === 'string' && response.default_file.trim()
      ? response.default_file.trim()
      : items[0]?.file || '';

  if (!keywordParam) {
    configFileMetaCache.set(`${targetSystem}:${ruleType}`, {
      defaultFile,
      items,
    });
  }

  return { items, defaultFile };
}

async function resolveDefaultConfigFile(ruleType, system) {
  const cacheKey = `${resolveSystem(system)}:${ruleType}`;
  const cached = configFileMetaCache.get(cacheKey);
  if (cached?.defaultFile) {
    return cached.defaultFile;
  }

  const response = await fetchConfigFileListResponse(ruleType, undefined, system);
  return response.defaultFile;
}

/**
 * 获取规则配置内容
 * @param {Object} options - 查询选项
 * @param {string} options.type - 配置类型（source/wpl/oml/knowledge/sink）
 * @param {string} options.file - 文件名（可选）
 * @returns {Promise<Object>} 配置内容
 */
export async function fetchRuleConfig(options) {
  const { type, file, system: rawSystem } = options;
  const system =
    type === RuleType.SOURCE_CONNECT || type === RuleType.SINK_CONNECT
      ? getSharedSystem()
      : resolveSystem(rawSystem);

  // Source 配置走真实后端：/api/config?rule_type=source&file=xxx.toml
  if (type === RuleType.SOURCE) {
    const targetFile = file || (await resolveDefaultConfigFile(RuleType.SOURCE, system));
    try {
      const response = await httpRequest.get('/config', {
        params: {
          system,
          rule_type: RuleType.SOURCE,
          file: targetFile,
        },
      });

      return {
        type,
        file: targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 后端返回 NOT_FOUND（连接配置文件不存在）时，视为尚未创建，返回空内容
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }
  
  // 连接配置（来源连接 / 输出连接）走真实后端：/api/config?rule_type=source_connect|sink_connect&file=xxx.toml
  if (type === 'source_connect' || type === 'sink_connect') {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何连接配置文件');
    }

    try {
      const response = await httpRequest.get('/config', {
        params: {
          system,
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        displayName: response?.display_name || response?.displayName || undefined,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      throw error;
    }
  }

  // 解析配置（parse）复用连接配置接口，使用 rule_type=parse
  if (type === RuleType.PARSE) {
    const targetFile = file || (await resolveDefaultConfigFile(RuleType.PARSE, system));
    try {
      const response = await httpRequest.get('/config', {
        params: {
          system,
          rule_type: RuleType.PARSE,
          file: targetFile,
        },
      });

      return {
        type,
        file: targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  // sink 配置走真实后端：/api/config?rule_type=sink&file=xxx.toml
  if (type === RuleType.SINK) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何 sink 配置文件');
    }

    try {
      const response = await httpRequest.get('/config', {
        params: {
          system,
          rule_type: RuleType.SINK,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      throw error;
    }
  }

  // wpl / oml / schema / rule 规则配置走通用规则接口：/api/config/rules
  if (
    type === RuleType.WPL ||
    type === RuleType.OML ||
    type === RuleType.WINDOWS ||
    type === RuleType.SCHEMA ||
    type === RuleType.RULE ||
    type === RuleType.SCENARIOS
  ) {
    const targetFile = file || (type === RuleType.WINDOWS ? 'windows.toml' : '');
    if (!targetFile) {
      throw new Error('当前未选择任何规则文件');
    }

    try {
      const response = await httpRequest.get('/config/rules', {
        params: {
          system,
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 规则文件不存在时，返回空内容，方便前端展示空白编辑器
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  // knowledge 配置走通用规则接口：/api/config/rules（返回多块内容）
  if (type === RuleType.KNOWLEDGE) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何数据集');
    }

    try {
      const response = await httpRequest.get('/config/rules', {
        params: {
          system,
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        config: response?.config || '',
        createSql: response?.create_sql || response?.createSql || '',
        insertSql: response?.insert_sql || response?.insertSql || '',
        data: response?.data || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 知识库配置不存在时，返回空内容，方便前端展示空白编辑器
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          config: '',
          createSql: '',
          insertSql: '',
          data: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  throw new Error(`不支持的配置类型: ${type}`);
}

/**
 * 获取规则列表
 * @param {Object} options - 查询选项
 * @param {string} options.type - 配置类型（wpl/oml/knowledge/connection）
 * @param {string} [options.keyword] - 文件名关键字（可选）
 * @returns {Promise<string[]>} 文件或数据集列表
 */
export async function fetchRuleFiles(options) {
  const { type, page, pageSize, keyword, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  // wpl / oml / schema / rule / knowledge 规则列表走后端：/api/config/rules/files
  if (
    type === RuleType.WPL ||
    type === RuleType.OML ||
    type === RuleType.WINDOWS ||
    type === RuleType.SCHEMA ||
    type === RuleType.RULE ||
    type === RuleType.SCENARIOS ||
    type === RuleType.KNOWLEDGE
  ) {
    const currentPage = typeof page === 'number' && page > 0 ? page : 1;
    const defaultPageSize = 15;
    const currentPageSize =
      typeof pageSize === 'number' && pageSize > 0 ? pageSize : defaultPageSize;

    const keywordParam =
      typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;

    const response = await httpRequest.get('/config/rules/files', {
      params: {
        system,
        rule_type: type,
        page: currentPage,
        page_size: currentPageSize,
        keyword: keywordParam,
      },
    });

    const items = Array.isArray(response?.items) ? response.items : [];
    const files = uniqueNames(items.map((item) => item.file));

    return {
      items: files,
      total: typeof response?.total === 'number' ? response.total : files.length,
      page: typeof response?.page === 'number' ? response.page : currentPage,
      pageSize:
        typeof response?.page_size === 'number' ? response.page_size : currentPageSize,
      meta: {
        wplParseFile: response?.meta?.wpl_parse_file || '',
        wplSampleFile: response?.meta?.wpl_sample_file || '',
        knowledgeConfigFile: response?.meta?.knowledge_config_file || '',
      },
    };
  }

  if (type === RuleType.SINK || type === RuleType.SOURCE || type === RuleType.PARSE) {
    const { items, defaultFile } = await fetchConfigFileListResponse(type, keyword, system);
    const normalizedItems = uniqueConnectionItems(items);

    return {
      items: normalizedItems,
      total: normalizedItems.length,
      page: 1,
      pageSize: normalizedItems.length || 1,
      meta: {
        defaultFile,
      },
    };
  }

  throw new Error(`不支持的规则类型: ${type}`);
}

// 创建规则文件（wpl / oml / knowledge）
export async function createRuleFile(options) {
  const { type, file, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  if (!type || !file) {
    throw new Error('创建规则文件时必须提供类型和文件名');
  }

  await httpRequest.post('/config/rules/files', {
    system,
    rule_type: type,
    file,
  });
}

// 删除规则文件（wpl / oml / knowledge）
export async function deleteRuleFile(options) {
  const { type, file, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  if (!type || !file) {
    throw new Error('删除规则文件时必须提供类型和文件名');
  }

  await httpRequest.delete('/config/rules/files', {
    params: {
      system,
      rule_type: type,
      file,
    },
  });
}

/**
 * 获取连接配置文件列表（来源 / 输出源），走真实后端接口
 * @param {Object} [options]
 * @param {string} [options.keyword] - 文件名关键字（可选）
 * @returns {Promise<{sources: Array<{file: string, displayName: string}>, sinks: Array<{file: string, displayName: string}>}>}
 */
export async function fetchConnectionFiles(options = {}) {
  const { keyword } = options;
  const system = getSharedSystem();

  const keywordParam =
    typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;

  const [sourceResponse, sinkResponse] = await Promise.all([
    httpRequest.get('/config/files', {
      params: {
        system,
        rule_type: 'source_connect',
        keyword: keywordParam,
      },
    }),
    httpRequest.get('/config/files', {
      params: {
        system,
        rule_type: 'sink_connect',
        keyword: keywordParam,
      },
    }),
  ]);

  const sourceItems = Array.isArray(sourceResponse?.items) ? sourceResponse.items : [];
  const sinkItems = Array.isArray(sinkResponse?.items) ? sinkResponse.items : [];

  const sources = uniqueConnectionItems(
    sourceItems.map((item) => ({
      file: item?.file,
      displayName: getConnectionDisplayName(item?.file, item?.display_name),
      sortOrder:
        typeof item?.sort_order === 'number' && item.sort_order >= 0
          ? item.sort_order
          : undefined,
    })),
  );
  const sinks = uniqueConnectionItems(
    sinkItems.map((item) => ({
      file: item?.file,
      displayName: getConnectionDisplayName(item?.file, item?.display_name),
      sortOrder:
        typeof item?.sort_order === 'number' && item.sort_order >= 0
          ? item.sort_order
          : undefined,
    })),
  );

  return { sources, sinks };
}

/**
 * 创建连接配置文件（来源 / 输出源）
 * @param {Object} options
 * @param {('source'|'sink')} options.category - 配置类别
 * @param {string} options.file - 文件名
 * @param {string} [options.displayName] - 展示名
 */
export async function createConnectionConfigFile(options) {
  const { category, file, displayName } = options;
  const system = getSharedSystem();

  if (!category || !file) {
    throw new Error('创建连接配置文件时必须提供类别和文件名');
  }

  await httpRequest.post('/config/files', {
    system,
    rule_type: category,
    file,
    display_name: displayName || undefined,
  });
}

/**
 * 创建配置文件（例如 sink）
 * @param {Object} options
 * @param {string} options.type - 配置类型
 * @param {string} options.file - 文件名
 * @param {string} [options.displayName] - 展示名
 */
export async function createConfigFile(options) {
  const { type, file, displayName, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  if (!type || !file) {
    throw new Error('创建配置文件时必须提供类型和文件名');
  }

  await httpRequest.post('/config/files', {
    system,
    rule_type: type,
    file,
    display_name: displayName || undefined,
  });
}

/**
 * 删除配置文件（例如 sink）
 * @param {Object} options
 * @param {string} options.type - 配置类型
 * @param {string} options.file - 文件名
 */
export async function deleteConfigFile(options) {
  const { type, file, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  if (!type || !file) {
    throw new Error('删除配置文件时必须提供类型和文件名');
  }

  await httpRequest.delete('/config/files', {
    params: {
      system,
      rule_type: type,
      file,
    },
  });
}

/**
 * 删除连接配置文件（来源 / 输出源）
 * @param {Object} options
 * @param {('source_connect'|'sink_connect')} options.category - 配置类别
 * @param {string} options.file - 文件名
 */
export async function deleteConnectionConfigFile(options) {
  const { category, file } = options;
  const system = getSharedSystem();

  if (!category || !file) {
    throw new Error('删除连接配置文件时必须提供类别和文件名');
  }

  await httpRequest.delete('/config/files', {
    params: {
      system,
      rule_type: category,
      file,
    },
  });
}

/**
 * 获取来源 / 输出配置模板列表
 * @param {'source'|'sink'} scope
 */
export async function fetchConfigTemplates(scope) {
  if (scope !== RuleType.SOURCE && scope !== RuleType.SINK) {
    throw new Error('配置模板 scope 仅支持 source 或 sink');
  }

  const system = getSharedSystem();
  const response = await httpRequest.get('/config/templates', {
    params: {
      system,
      scope,
    },
  });

  return {
      items: Array.isArray(response?.items)
      ? response.items.map((item) => ({
          scope: item?.scope,
          templateFile: item?.template_file,
          templateId: item?.template_id,
          displayName: item?.display_name,
          connect: item?.connect,
          connectorType: item?.connector_type,
          connectorTypeDisplayName: item?.connector_type_display_name,
          requiredFields: Array.isArray(item?.required_fields) ? item.required_fields : [],
          insertedFields: Array.isArray(item?.inserted_fields) ? item.inserted_fields : [],
          omittedFields: Array.isArray(item?.omitted_fields) ? item.omitted_fields : [],
          fields: Array.isArray(item?.fields)
            ? item.fields.map((field) => ({
                name: field?.name,
                required: Boolean(field?.required),
                defaultValue: field?.default_value,
                advanced: Boolean(field?.advanced),
              }))
            : [],
        }))
      : [],
  };
}

/**
 * 渲染来源 / 输出配置模板片段
 * @param {Object} options
 * @param {'source'|'sink'} options.scope
 * @param {string} options.templateId
 * @param {string} options.content
 */
export async function renderConfigTemplate(options) {
  const { scope, templateId, content } = options || {};

  if ((scope !== RuleType.SOURCE && scope !== RuleType.SINK) || !templateId) {
    throw new Error('渲染配置模板时必须提供有效的 scope 和 templateId');
  }

  const system = getSharedSystem();
  const response = await httpRequest.post('/config/templates/render', {
    system,
    scope,
    template_id: templateId,
    content: content || '',
  });

  return {
    scope: response?.scope,
    templateFile: response?.template_file,
    templateId: response?.template_id,
    displayName: response?.display_name,
    connect: response?.connect,
    connectorType: response?.connector_type,
    connectorTypeDisplayName: response?.connector_type_display_name,
    instanceName: response?.instance_name,
    requiredFields: Array.isArray(response?.required_fields) ? response.required_fields : [],
    insertedFields: Array.isArray(response?.inserted_fields) ? response.inserted_fields : [],
    omittedFields: Array.isArray(response?.omitted_fields) ? response.omitted_fields : [],
    warnings: Array.isArray(response?.warnings) ? response.warnings : [],
    snippet: response?.snippet || '',
    content: response?.content || '',
  };
}

/**
 * 校验规则配置
 * @param {Object} options - 校验选项
 * @param {string} options.type - 配置类型
 * @param {string} options.content - 配置内容
 * @returns {Promise<Object>} 校验结果
 */
export async function validateRuleConfig(options) {
  const { type, file, content, system: rawSystem } = options;
  const system =
    type === RuleType.SOURCE_CONNECT || type === RuleType.SINK_CONNECT
      ? getSharedSystem()
      : resolveSystem(rawSystem);

  // 所有类型统一走真实后端校验：POST /api/config/rules/validate

  // 若未显式传入文件名，则根据类型给一个合理的默认值
  let targetFile = file;
  if (!targetFile) {
    if (type === RuleType.SOURCE) {
      targetFile = await resolveDefaultConfigFile(RuleType.SOURCE, system);
    } else if (type === RuleType.PARSE) {
      targetFile = await resolveDefaultConfigFile(RuleType.PARSE, system);
    } else if (type === RuleType.WINDOWS) {
      targetFile = 'windows.toml';
    } else {
      targetFile = `${type}.toml`;
    }
  }

  const currentContent = content || '';

  const response = await httpRequest.post('/config/rules/validate', {
    system,
    rule_type: type,
    file: targetFile,
    content: currentContent,
  });

  const lineCount = currentContent ? currentContent.split('\n').length : 0;

  return {
    filename: targetFile,
    lines: lineCount,
    valid: Boolean(response?.valid),
    warnings: 0,
    errors: [],
    message: response?.message,
    details: response?.details || [],
  };
}

/**
 * 保存规则配置
 * @param {Object} options - 保存选项
 * @param {string} options.type - 配置类型
 * @param {string} options.file - 文件名
 * @param {string} options.content - 配置内容
 * @returns {Promise<Object>} 保存结果
 */
export async function saveRuleConfig(options) {
  const { type, file, content, system: rawSystem } = options;
  const system =
    type === RuleType.SOURCE_CONNECT || type === RuleType.SINK_CONNECT
      ? getSharedSystem()
      : resolveSystem(rawSystem);

  // Source 配置走真实后端保存：POST /api/config
  if (type === RuleType.SOURCE) {
    const targetFile = file || (await resolveDefaultConfigFile(RuleType.SOURCE, system));

    await httpRequest.post('/config', {
      system,
      rule_type: RuleType.SOURCE,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // 连接配置（来源连接 / 输出连接）走真实后端保存：POST /api/config
  if (type === 'source_connect' || type === 'sink_connect') {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何连接配置文件');
    }

    await httpRequest.post('/config', {
      system,
      rule_type: type,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // 解析配置（parse）走真实后端保存：POST /api/config，固定文件 wparse.toml
  if (type === RuleType.PARSE) {
    const targetFile = file || (await resolveDefaultConfigFile(RuleType.PARSE, system));

    await httpRequest.post('/config', {
      system,
      rule_type: RuleType.PARSE,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // sink 配置走真实后端保存：POST /api/config
  if (type === RuleType.SINK) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何 sink 配置文件');
    }

    await httpRequest.post('/config', {
      system,
      rule_type: RuleType.SINK,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // wpl / oml / schema / rule 规则保存走通用规则接口：POST /api/config/rules/save
  if (
    type === RuleType.WPL ||
    type === RuleType.OML ||
    type === RuleType.WINDOWS ||
    type === RuleType.SCHEMA ||
    type === RuleType.RULE ||
    type === RuleType.SCENARIOS
  ) {
    const targetFile = file || (type === RuleType.WINDOWS ? 'windows.toml' : '');
    if (!targetFile) {
      throw new Error('当前未选择任何规则文件');
    }

    await httpRequest.post('/config/rules/save', {
      system,
      rule_type: type,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  throw new Error(`不支持的保存类型: ${type}`);
}

// 保存知识库规则配置（knowledge 类型）
export async function saveKnowledgeRule(options) {
  const { file, config, createSql, insertSql, data, system: rawSystem } = options;
  const system = resolveSystem(rawSystem);

  if (!file) {
    throw new Error('当前未选择任何数据集');
  }

  await httpRequest.post(
    '/config/knowledge/save',
    {
      file,
      system,
      config: config ?? '',
      create_sql: createSql ?? '',
      insert_sql: insertSql ?? '',
      data: data ?? '',
    },
    {
      timeout: KNOWLEDGE_SAVE_TIMEOUT_MS,
    },
  );

  const fileSize = (config || '').length + (createSql || '').length + (insertSql || '').length + (data || '').length;

  return {
    success: true,
    fileSize,
    message: '保存成功',
  };
}

export async function fetchKnowdbConfig(system) {
  const response = await httpRequest.get('/config/knowledge/knowdb', {
    params: { system: resolveSystem(system) },
  });
  return {
    file: response?.file || 'knowdb.toml',
    content: response?.content || '',
    lastModified: response?.last_modified || null,
  };
}

export async function saveKnowdbConfig(content, system) {
  await httpRequest.post('/config/knowledge/knowdb', {
    system: resolveSystem(system),
    content: content ?? '',
  });
  return {
    success: true,
    message: '保存成功',
  };
}


/**
 * 获取调试页知识库数据源列表。
 * @returns {Promise<Array<{tagName: string, sourceKind: string, label: string, suggestedSql: string}>>} 知识库表或外部 provider 信息
 */
export async function fetchDebugKnowledgeDatasets() {
  const response = await httpRequest.get('/debug/knowledge/status');
  if (!Array.isArray(response)) {
    return [];
  }

  return response
    .filter((item) => item && typeof item === 'object' && item?.is_active !== false)
    .map((item) => {
      const tagName = item?.tag_name;
      if (!tagName) {
        return null;
      }

      const sourceKind = String(item?.source_kind || '').trim().toLowerCase();
      if (sourceKind !== 'provider' && sourceKind !== 'local') {
        return null;
      }

      return {
        tagName,
        sourceKind,
        label: String(item?.label || tagName),
        suggestedSql: String(item?.suggested_sql || ''),
      };
    })
    .filter(Boolean);
}

const extractBackendErrorMessage = (error, fallbackMessage = '查询失败') => {
  const responseData = error?.response?.data || error?.data || error?.responseData;
  const backendError = responseData?.error;
  const details = backendError?.details || backendError?.detail;

  if (typeof details === 'string' && details.trim()) {
    return details;
  }
  if (details && typeof details === 'object') {
    const detailText = details.reason || details.message || JSON.stringify(details);
    if (detailText) {
      return detailText;
    }
  }
  if (backendError?.message) {
    return backendError.message;
  }
  if (error?.message) {
    return error.message;
  }
  return fallbackMessage;
};


/**
 * 执行知识库 SQL 查询
 * @param {string|{tagName: string, sourceKind?: string}} table - 当前选择的数据源信息
 * @param {string} sql - SQL 查询语句
 * @returns {Promise<{fields: Array, columns: Array}>} 处理后的查询结果
 */
export async function executeKnowledgeSql(table, sql) {
  const tableName =
    (table && typeof table === 'object' ? table.tagName || table.value : table) || '';
  const sourceKind =
    table && typeof table === 'object' ? table.sourceKind || '' : '';
  let response;
  try {
    response = await httpRequest.post('/debug/knowledge/query', {
      table: tableName,
      source_kind: sourceKind,
      sql,
    });
  } catch (error) {
    const wrapped = new Error(extractBackendErrorMessage(error));
    wrapped.code = error?.response?.data?.error?.code || error?.code;
    wrapped.responseData = error?.response?.data || error?.data || error?.responseData;
    throw wrapped;
  }

  // 调试接口格式：{ success, columns: string[], rows: string[][], total }
  if (response?.success && Array.isArray(response.columns) && Array.isArray(response.rows)) {
    const columns = response.columns.map((header) => ({
      title: header,
      dataIndex: header,
      key: header,
    }));

    const fields = response.rows.map((row, rowIndex) => {
      const rowData = { key: rowIndex };
      response.columns.forEach((header, columnIndex) => {
        rowData[header] = row[columnIndex] ?? '';
      });
      return rowData;
    });

    return { fields, columns };
  }

  // 兼容多种响应格式：
  // 1. 完整格式: { code: 200, msg: "success", data: [...] }
  // 2. 已解包格式: [...] (直接是 data 数组)
  const dataArray = Array.isArray(response) 
    ? response 
    : (response && response.code === 200 && Array.isArray(response.data) ? response.data : null);

  if (dataArray && dataArray.length > 0) {
    // 新后端返回格式: 二维数组，每行是字段对象数组
    // [[{meta, name, value: {Digit|Chars: x}}, ...], ...]
    const firstRow = dataArray[0];
    
    // 检测是否为新格式（数组的数组，且内部对象有 name 和 value 属性）
    const isNewFormat = Array.isArray(firstRow) && 
      firstRow.length > 0 && 
      firstRow[0] && 
      typeof firstRow[0].name === 'string' && 
      firstRow[0].value !== undefined;

    if (isNewFormat) {
      // 从第一行提取列名
      const headers = firstRow.map((field) => field.name);

      // 生成列定义
      const columns = headers.map((header) => ({
        title: header,
        dataIndex: header,
        key: header,
      }));

      // 解析每行数据
      const fields = dataArray.map((row, rowIndex) => {
        const rowData = { key: rowIndex };
        row.forEach((field) => {
          // 从 value 对象中提取实际值（Digit 或 Chars）
          const valueObj = field.value;
          let actualValue = valueObj;
          if (valueObj && typeof valueObj === 'object') {
            // 取对象的第一个值（Digit 或 Chars 的值）
            actualValue = Object.values(valueObj)[0];
          }
          rowData[field.name] = actualValue;
        });
        return rowData;
      });

      return { fields, columns };
    }
  }

  if (response && response.code !== 200) {
    throw new Error(response.msg || '查询失败');
  } 
  return { fields: [], columns: [] };
}
