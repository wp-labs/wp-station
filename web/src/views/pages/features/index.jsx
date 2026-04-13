import React, { useEffect, useState } from 'react';

import {
  DEFAULT_DATA_COLLECT_URL,
  fetchDataCollectConfig,
} from '../../../services/features';

/**
 * 数据采集页面
 * 功能：展示数据采集能力示意图
 * 对应原型：pages/views/data-collect.html
 */
function FeaturesPage() {
  const [iframeUrl, setIframeUrl] = useState(DEFAULT_DATA_COLLECT_URL);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  useEffect(() => {
    let active = true;

    (async () => {
      try {
        const config = await fetchDataCollectConfig();
        if (active && config?.data_collect_url) {
          setIframeUrl(config.data_collect_url);
        }
      } catch (err) {
        console.error('加载数据采集配置失败:', err);
        if (active) {
          setError('数据采集配置加载失败，已回退默认地址。');
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();

    return () => {
      active = false;
    };
  }, []);

  return (
    <section className="page-panels">
      <article className="panel is-visible">
        <div
          style={{
            width: '100%',
            height: '100%',
            borderRadius: '16px',
            overflow: 'hidden',
            position: 'relative',
          }}
        >
          {(loading || error) && (
            <div
              style={{
                position: 'absolute',
                top: 12,
                left: 12,
                right: 12,
                zIndex: 2,
                padding: '8px 12px',
                borderRadius: 8,
                background: 'rgba(0, 0, 0, 0.6)',
                color: '#fff',
                fontSize: 14,
                lineHeight: 1.4,
              }}
            >
              {loading ? '正在加载数据采集配置…' : error}
            </div>
          )}
          <iframe
            src={iframeUrl}
            title="数据采集监控"
            style={{
              width: '100%',
              height: '100%',
              border: 'none',
            }}
          />
        </div>
      </article>
    </section>
  );
}

export default FeaturesPage;
