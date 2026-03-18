// Kodo Language Extension for VS Code
// Provides syntax highlighting and LSP client integration.

const vscode = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");
const { execFileSync } = require("child_process");

/** @type {LanguageClient | undefined} */
let client;

/** @type {vscode.OutputChannel} */
let outputChannel;

/**
 * Check whether the kodoc binary exists and is executable.
 * @param {string} serverPath
 * @returns {boolean}
 */
function isServerAvailable(serverPath) {
  try {
    execFileSync(serverPath, ["--version"], {
      timeout: 5000,
      stdio: "pipe",
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * @param {vscode.ExtensionContext} context
 */
async function activate(context) {
  outputChannel = vscode.window.createOutputChannel("Kodo Language Server");
  context.subscriptions.push(outputChannel);

  const config = vscode.workspace.getConfiguration("kodo");
  const serverPath = config.get("serverPath", "kodoc");

  // Verify the server binary is reachable before attempting to start.
  if (!isServerAvailable(serverPath)) {
    const msg =
      `Kodo language server not found: "${serverPath}". ` +
      "Install kodoc and ensure it is on your PATH, or set kodo.serverPath in settings.";
    outputChannel.appendLine(msg);
    vscode.window
      .showWarningMessage(msg, "Open Settings")
      .then((selection) => {
        if (selection === "Open Settings") {
          vscode.commands.executeCommand(
            "workbench.action.openSettings",
            "kodo.serverPath"
          );
        }
      });
    return;
  }

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
    outputChannel,
  };

  client = new LanguageClient(
    "kodoLanguageServer",
    "Kodo Language Server",
    serverOptions,
    clientOptions
  );

  try {
    await client.start();
    outputChannel.appendLine("Kodo Language Server started successfully.");
  } catch (err) {
    const errorMsg = `Failed to start Kodo Language Server: ${err.message || err}`;
    outputChannel.appendLine(errorMsg);
    vscode.window.showErrorMessage(errorMsg);
    client = undefined;
  }
}

async function deactivate() {
  if (client) {
    await client.stop();
  }
}

module.exports = { activate, deactivate };
