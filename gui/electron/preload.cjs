const { contextBridge, ipcRenderer } = require('electron');
contextBridge.exposeInMainWorld('deployer', {
  snapshot: () => ipcRenderer.invoke('snapshot'),
  saveEnv: (payload) => ipcRenderer.invoke('save-env', payload),
  operation: (payload) => ipcRenderer.invoke('operation', payload),
  removeProduct: (id) => ipcRenderer.invoke('remove-product', id)
});
