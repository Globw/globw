// VS Code support for Easier: run button / F5, and errors underlined on open and save.

const vscode = require('vscode');
const fs = require('fs');
const { execFile } = require('child_process');
const { parseErrors } = require('./parse');

const LOCAL_BUILD = 'C:\\Users\\Utente\\easier\\target\\release\\easier.exe';

function easierPath() {
    const configured = vscode.workspace.getConfiguration('easier').get('path');
    if (configured) return configured;
    return fs.existsSync(LOCAL_BUILD) ? LOCAL_BUILD : 'easier';
}

/** Runs `easier --check` on the saved file and shows its errors as diagnostics. */
function check(doc, diagnostics) {
    if (doc.languageId !== 'easier' || doc.uri.scheme !== 'file') return;
    execFile(easierPath(), ['--check', doc.fileName], { windowsHide: true }, (err, _stdout, stderr) => {
        if (err && err.code === 'ENOENT') {
            vscode.window.showErrorMessage('Easier: easier.exe not found. Set "easier.path" in the settings.');
            return;
        }
        const items = parseErrors(stderr || '').map((e) => {
            const range =
                e.col === null
                    ? doc.lineAt(Math.min(e.line, doc.lineCount - 1)).range
                    : new vscode.Range(e.line, e.col, e.line, e.col + (e.len || 1));
            const text = e.hint ? `${e.message}\nHint: ${e.hint}` : e.message;
            const d = new vscode.Diagnostic(range, text, vscode.DiagnosticSeverity.Error);
            d.source = 'easier';
            return d;
        });
        diagnostics.set(doc.uri, items);
    });
}

/** Saves the file and runs it in an "Easier" terminal, where input() can be answered. */
async function run() {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'easier') return;
    const doc = editor.document;
    if (doc.isUntitled) {
        vscode.window.showWarningMessage('Easier: save the file as .ez before running it.');
        return;
    }
    await doc.save();
    let terminal = vscode.window.terminals.find((t) => t.name === 'Easier');
    if (!terminal) {
        terminal = vscode.window.createTerminal({ name: 'Easier', shellPath: 'powershell.exe', shellArgs: ['-NoLogo'] });
    }
    terminal.show();
    terminal.sendText(`& "${easierPath()}" "${doc.fileName}"`);
}

function activate(context) {
    const diagnostics = vscode.languages.createDiagnosticCollection('easier');
    context.subscriptions.push(
        diagnostics,
        vscode.commands.registerCommand('easier.run', run),
        vscode.workspace.onDidOpenTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidSaveTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri))
    );
    vscode.workspace.textDocuments.forEach((doc) => check(doc, diagnostics));
}

function deactivate() {}

module.exports = { activate, deactivate };
