import { useRef } from 'react';
import { useRequest } from 'ahooks';
import { Empty, Spin } from 'antd';
import gitlabService from '@/services/gitlab';
import MonacoEditor from '@/views/components/MonacoEditor';

export default function GitlabPage() {
  const editorRef = useRef(null);
  const { data, loading, error } = useRequest(gitlabService.version);

  if (loading) return <Spin />;

  if (error) return <Empty description="加载分类失败" />;

  return (
    <div>
      <h1>Gitlab Page</h1>
      <MonacoEditor ref={editorRef} value={JSON.stringify(data, null, 2)} language="json" theme="vs-dark" />
    </div>
  );
}
