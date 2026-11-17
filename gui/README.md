# Frappe Deployer GUI

Frontend desktop em Electron + React para o mesmo workspace usado pela TUI Ratatui.

## Desenvolvimento

Na raiz do repositório:

```bash
cd gui
npm install
npm run dev
```

A GUI lê `products/*`, mascara segredos por padrão, salva `.env`/`build.env` com backup e executa operações por `common/dc.sh`. `down` e remoção de produto pedem confirmação nativa do Electron. Volumes Docker não são removidos pela GUI.

Para uma build do renderer:

```bash
npm run build
npm start
```

A GUI pressupõe que o workspace contenha `common/dc.sh` e `products/`. Ela não substitui o fluxo completo da modal de criação da TUI; a criação complexa continua centralizada no Rust para evitar duplicação das regras de validação.
