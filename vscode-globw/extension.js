// VS Code support for Globw: run button / F5, and errors underlined on open and save.

const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const { execFile } = require('child_process');
const { parseErrors } = require('./parse');

const LOCAL_BUILD = 'C:\\Users\\Utente\\globw\\target\\release\\globw.exe';

function globwPath() {
    const configured = vscode.workspace.getConfiguration('globw').get('path');
    if (configured) return configured;
    return fs.existsSync(LOCAL_BUILD) ? LOCAL_BUILD : 'globw';
}

/** The line of `doc` that imports module `name` (the first line if none does). */
function importLine(doc, name) {
    // module names are made of letters, digits and _, so they need no escaping
    const re = new RegExp(`^\\s*(lazy\\s+)?(import\\b.*\\b${name}\\b|from\\s+${name}\\b)`);
    for (let i = 0; i < doc.lineCount; i++) {
        if (re.test(doc.lineAt(i).text)) return i;
    }
    return 0;
}

/** Runs `globw --check` on the saved file and shows its errors as diagnostics. */
function check(doc, diagnostics) {
    if (doc.languageId !== 'globw' || doc.uri.scheme !== 'file') return;
    execFile(globwPath(), ['--check', doc.fileName], { windowsHide: true }, (err, _stdout, stderr) => {
        if (err && err.code === 'ENOENT') {
            vscode.window.showErrorMessage('Globw: globw.exe not found. Set "globw.path" in the settings.');
            return;
        }
        // errors of this file, and of the modules it imports (shown in their own file
        // and, once per module, on the line of this file that imports it)
        const byFile = new Map([[doc.uri.toString(), { uri: doc.uri, items: [] }]]);
        const marked = new Set();
        for (const e of parseErrors(stderr || '')) {
            const range =
                e.col === null
                    ? new vscode.Range(e.line, 0, e.line, 1000)
                    : new vscode.Range(e.line, e.col, e.line, e.col + (e.len || 1));
            const text = e.hint ? `${e.message}\nHint: ${e.hint}` : e.message;
            const d = new vscode.Diagnostic(range, text, vscode.DiagnosticSeverity.Error);
            d.source = 'globw';
            const uri = e.file ? vscode.Uri.file(path.join(path.dirname(doc.fileName), e.file)) : doc.uri;
            if (!byFile.has(uri.toString())) byFile.set(uri.toString(), { uri, items: [] });
            byFile.get(uri.toString()).items.push(d);
            if (e.file && !marked.has(e.file)) {
                marked.add(e.file);
                const line = importLine(doc, e.file.replace(/\.gw$/, ''));
                const where = new vscode.Range(line, 0, line, doc.lineAt(line).text.length);
                const note = new vscode.Diagnostic(where, `error in ${e.file}: ${e.message}`, vscode.DiagnosticSeverity.Error);
                note.source = 'globw';
                byFile.get(doc.uri.toString()).items.push(note);
            }
        }
        for (const { uri, items } of byFile.values()) diagnostics.set(uri, items);
    });
}

/** Saves the file and runs it in a "Globw" terminal, where input() can be answered. */
async function run() {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'globw') return;
    const doc = editor.document;
    if (doc.isUntitled) {
        vscode.window.showWarningMessage('Globw: save the file as .gw before running it.');
        return;
    }
    await doc.save();
    let terminal = vscode.window.terminals.find((t) => t.name === 'Globw');
    if (!terminal) {
        terminal = vscode.window.createTerminal({ name: 'Globw', shellPath: 'powershell.exe', shellArgs: ['-NoLogo'] });
    }
    terminal.show();
    terminal.sendText(`& "${globwPath()}" "${doc.fileName}"`);
}

function activate(context) {
    const diagnostics = vscode.languages.createDiagnosticCollection('globw');
    context.subscriptions.push(
        diagnostics,
        vscode.commands.registerCommand('globw.run', run),
        vscode.workspace.onDidOpenTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidSaveTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri))
    );
    vscode.workspace.textDocuments.forEach((doc) => check(doc, diagnostics));
}

function deactivate() {}

module.exports = { activate, deactivate };
