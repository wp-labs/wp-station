import { gitlabRequest } from '@/services/request';

// Gitlab API 相关服务
const service = {
  version: async () => {
    return await gitlabRequest.get('/version');
  },
};

export default service;
