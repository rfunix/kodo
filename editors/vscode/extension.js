// Kodo Language Extension for VS Code
// Provides syntax highlighting and LSP client integration.

const vscode = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

let client;

function activate(context) {
  const config = vscode.workspace.getConfiguration("kodo");
  const serverPath = config.get("serverPath", "kodoc");

  const serverOptions = {
    command: serverPath,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "kodo" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.ko"),
    },
  };

  client = new LanguageClient(
    "kodoLanguageServer",
    "Kodo Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

function deactivate() {
  if (client) {
    return client.stop();
  }
}

module.exports = { activate, deactivate };
