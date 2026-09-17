import httpRequest from './request';
import { resolveSystem } from './system';

const API_BASE = '/api';
const IMPORTABLE_FOLDER_NAMES = new Set(['conf', 'connectors', 'models', 'topology']);

function getOperatorHeader() {
  const username = sessionStorage.getItem('username');
  return username ? { 'X-Operator': encodeURIComponent(username) } : {};
}

async function parseErrorResponse(response, fallback) {
  try {
    const body = await response.json();
    return body?.error?.message || body?.error?.details || fallback;
  } catch (_) {
    return fallback;
  }
}

export async function importProjectArchive(file, system) {
  const targetSystem = resolveSystem(system);
  const response = await fetch(`${API_BASE}/project/import/archive?system=${encodeURIComponent(targetSystem)}`, {
    method: 'POST',
    headers: {
      ...getOperatorHeader(),
      'Content-Type': 'application/octet-stream',
      'X-File-Name': encodeURIComponent(file.name),
    },
    body: file,
  });

  if (!response.ok) {
    throw new Error(await parseErrorResponse(response, '导入配置失败'));
  }

  return response.json();
}

/**
 * 将浏览器选择的文件夹打成 tar 归档，复用后端现有的归档预检/确认链路。
 * 只保留当前目录或当前目录直接子目录中的核心目录（conf/connectors/models/topology）及其下的文件，
 * 这样只支持选择项目根目录或直接选择单个核心目录。
 * 这里不把文件内容读入 JS 内存，File 对象会作为 Blob part 直接交给浏览器构造归档。
 */
export function buildProjectFolderArchive(files) {
  const sourceFiles = Array.from(files || [])
    .map((entry) => {
      const file = entry?.file || entry;
      const rawPath = entry?.relativePath || file?.webkitRelativePath || file?.name;
      if (!file || typeof file.size !== 'number' || file.size < 0) return null;

      const relativePath = normalizeFolderEntryPath(rawPath);
      const importPath = getImportableFolderPath(relativePath);
      return importPath ? { file, relativePath: importPath } : null;
    })
    .filter(Boolean);
  if (sourceFiles.length === 0) {
    throw new Error('选择的文件夹中未找到 conf、connectors、models 或 topology 文件');
  }

  const parts = [];
  sourceFiles.forEach(({ file, relativePath }) => {
    const header = buildTarHeader(relativePath, file.size, file.lastModified);
    parts.push(header, file);

    const remainder = file.size % 512;
    if (remainder !== 0) {
      parts.push(new Uint8Array(512 - remainder));
    }
  });

  // tar 规范要求以两个 512 字节的空块结束。
  parts.push(new Uint8Array(1024));
  return new File(parts, `wp-station-folder-${Date.now()}.tar`, {
    type: 'application/x-tar',
  });
}

function normalizeFolderEntryPath(rawPath) {
  const normalized = String(rawPath || '').replaceAll('\\', '/').replace(/^\/+/, '');
  const segments = normalized.split('/').filter(Boolean);
  if (
    segments.length === 0 ||
    segments.some((segment) => segment === '.' || segment === '..' || segment.includes('\0'))
  ) {
    throw new Error('所选文件夹包含不安全的文件路径');
  }
  return segments.join('/');
}

function getImportableFolderPath(relativePath) {
  const segments = relativePath.split('/');
  // 只允许“当前目录就是配置目录”或“当前目录的直接子目录是配置目录”。
  // 不向更深层级搜索，避免把 .wfusion-validation 等辅助目录里的 models
  // 误识别为项目配置目录。
  if (IMPORTABLE_FOLDER_NAMES.has(segments[0])) {
    return segments.join('/');
  }
  if (segments.length > 1 && IMPORTABLE_FOLDER_NAMES.has(segments[1])) {
    return segments.slice(1).join('/');
  }
  return null;
}

function buildTarHeader(filePath, size, lastModified) {
  const header = new Uint8Array(512);
  const splitIndex = filePath.length > 100 ? filePath.lastIndexOf('/', 154) : -1;
  const name = splitIndex >= 0 ? filePath.slice(splitIndex + 1) : filePath;
  const prefix = splitIndex >= 0 ? filePath.slice(0, splitIndex) : '';
  if (name.length > 100 || prefix.length > 155) {
    throw new Error(`文件路径过长，无法导入: ${filePath}`);
  }

  writeTarString(header, 0, 100, name);
  writeTarOctal(header, 100, 8, 0o644);
  writeTarOctal(header, 108, 8, 0);
  writeTarOctal(header, 116, 8, 0);
  writeTarOctal(header, 124, 12, size);
  writeTarOctal(header, 136, 12, Math.floor((lastModified || Date.now()) / 1000));
  header.fill(0x20, 148, 156);
  header[156] = 0x30; // 普通文件
  writeTarString(header, 257, 6, 'ustar');
  writeTarString(header, 263, 2, '00');
  writeTarString(header, 265, 32, 'wp-station');
  writeTarString(header, 297, 32, 'wp-station');

  const checksum = header.reduce((sum, value) => sum + value, 0);
  writeTarOctal(header, 148, 8, checksum);
  return header;
}

function writeTarString(target, offset, length, value) {
  const bytes = new TextEncoder().encode(value).slice(0, length);
  target.set(bytes, offset);
}

function writeTarOctal(target, offset, length, value) {
  const digits = Math.max(0, Math.floor(Number(value) || 0)).toString(8);
  const content = `${digits}`.padStart(length - 1, '0');
  const bytes = new TextEncoder().encode(content.slice(-(length - 1)));
  target.set(bytes, offset);
  target[offset + length - 1] = 0;
}

export async function confirmProjectArchiveImport(importId, system) {
  const response = await fetch(`${API_BASE}/project/import/archive/confirm`, {
    method: 'POST',
    headers: {
      ...getOperatorHeader(),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ system: resolveSystem(system), import_id: importId }),
  });

  if (!response.ok) {
    throw new Error(await parseErrorResponse(response, '确认导入配置失败'));
  }

  return response.json();
}

export async function importProjectFromFiles({ sourceDir, system }) {
  const response = await httpRequest.post('/project/import', {
    system: resolveSystem(system),
    source_dir: sourceDir,
  });
  return response?.summary ? response : response?.data || response;
}

export async function exportProjectArchive(system) {
  const targetSystem = resolveSystem(system);
  const response = await fetch(`${API_BASE}/project/export/archive?system=${encodeURIComponent(targetSystem)}`, {
    method: 'GET',
    headers: getOperatorHeader(),
  });

  if (!response.ok) {
    throw new Error(await parseErrorResponse(response, '导出配置包失败'));
  }

  const blob = await response.blob();
  const disposition = response.headers.get('content-disposition') || '';
  const match = disposition.match(/filename="?([^";]+)"?/i);
  const archivePrefix = targetSystem === 'wfusion' ? 'wfusion' : 'wparse';
  const fileName = match?.[1]
    || `${archivePrefix}-${Math.floor(Date.now() / 1000)}.tar.gz`;
  return { blob, fileName };
}

export function downloadBlob(blob, fileName) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}
