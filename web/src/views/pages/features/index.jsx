import React, { useEffect } from 'react';
import {
  DEFAULT_DATA_COLLECT_URL,
  fetchDataCollectConfig,
} from '@/services/features';

/**
 * 运行监控跳转页
 * 保留旧路由 /features，进入后直接跳到外部监控页面。
 */
function FeaturesPage() {
  useEffect(() => {
    let active = true;

    (async () => {
      try {
        const config = await fetchDataCollectConfig();
        if (active) {
          window.location.assign(config?.data_collect_url || DEFAULT_DATA_COLLECT_URL);
        }
      } catch (_error) {
        if (active) {
          window.location.assign(DEFAULT_DATA_COLLECT_URL);
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
        <header className="panel-header">
          <h2>运行监控</h2>
        </header>
        <section className="panel-body">
          <p style={{ margin: 0, color: 'var(--muted)' }}>正在跳转到运行监控页面...</p>
        </section>
      </article>
    </section>
  );
}

export default FeaturesPage;
