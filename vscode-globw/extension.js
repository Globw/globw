// VS Code support for Globw: run button / F5, errors underlined on open and save
// (via `gw --check --json`), and completion + hover for the builtins (from
// `gw --builtins`).

const vscode = require('vscode');
const fs = require('fs');
const path = require('path');
const { execFile } = require('child_process');

const EXE = process.platform === 'win32' ? 'gw.exe' : 'gw';

// Keywords offered in completion alongside the builtins.
const KEYWORDS = [
    'variables', 'function', 'procedure', 'return', 'end', 'if', 'else', 'for', 'from', 'to', 'step',
    'while', 'do', 'break', 'continue', 'import', 'lazy', 'as', 'of', 'list', 'dict', 'set', 'to',
    'int', 'float', 'string', 'bool', 'nungw', 'new', 'class', 'extends', 'try', 'catch', 'always',
    'and', 'or', 'not', 'true', 'false', 'none', 'in', 'gives', 'output', 'input', 'append', 'remove', 'length',
];

// Map of builtin name -> signature string, loaded once from `gw --builtins`.
let catalogue = new Map();

function globwPath() {
    const configured = vscode.workspace.getConfiguration('globw').get('path');
    if (configured) return configured;
    // a local dev build inside the opened workspace, if there is one
    for (const folder of vscode.workspace.workspaceFolders || []) {
        const local = path.join(folder.uri.fsPath, 'target', 'release', EXE);
        if (fs.existsSync(local)) return local;
    }
    return EXE; // otherwise rely on PATH
}

/** Loads the builtin catalogue (name + signature) for completion and hover. */
function loadCatalogue() {
    execFile(globwPath(), ['--builtins'], { windowsHide: true, maxBuffer: 8 * 1024 * 1024 }, (err, stdout) => {
        if (err) return; // older gw without --builtins, or not found: just skip
        try {
            const map = new Map();
            for (const b of JSON.parse(stdout || '[]')) map.set(b.name, b.sig);
            catalogue = map;
        } catch {
            /* leave the previous catalogue */
        }
    });
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

/** Runs `gw --check --json` on the file and shows its errors as diagnostics. */
function check(doc, diagnostics) {
    if (doc.languageId !== 'globw' || doc.uri.scheme !== 'file') return;
    execFile(globwPath(), ['--check', '--json', doc.fileName], { windowsHide: true }, (err, stdout) => {
        if (err && err.code === 'ENOENT') {
            vscode.window.showErrorMessage('Globw: the gw executable was not found. Set "globw.path" in the settings, or add gw to your PATH.');
            return;
        }
        let errors;
        try {
            errors = JSON.parse(stdout || '[]');
        } catch {
            return; // an older gw without --json, or unexpected output
        }
        // errors of this file, and of the modules it imports (shown in their own file
        // and, once per module, on the line of this file that imports it)
        const byFile = new Map([[doc.uri.toString(), { uri: doc.uri, items: [] }]]);
        const marked = new Set();
        for (const e of errors) {
            const line = Math.max(0, (e.line || 1) - 1);
            const range =
                !e.col
                    ? new vscode.Range(line, 0, line, 1000)
                    : new vscode.Range(line, e.col - 1, line, e.col - 1 + (e.len || 1));
            const text = e.hint ? `${e.message}\nHint: ${e.hint}` : e.message;
            const d = new vscode.Diagnostic(range, text, vscode.DiagnosticSeverity.Error);
            d.source = 'globw';
            const uri = e.file ? vscode.Uri.file(path.join(path.dirname(doc.fileName), e.file)) : doc.uri;
            if (!byFile.has(uri.toString())) byFile.set(uri.toString(), { uri, items: [] });
            byFile.get(uri.toString()).items.push(d);
            if (e.file && !marked.has(e.file)) {
                marked.add(e.file);
                const il = importLine(doc, e.file.replace(/\.gw$/, ''));
                const where = new vscode.Range(il, 0, il, doc.lineAt(il).text.length);
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
        const opts =
            process.platform === 'win32'
                ? { name: 'Globw', shellPath: 'powershell.exe', shellArgs: ['-NoLogo'] }
                : { name: 'Globw' };
        terminal = vscode.window.createTerminal(opts);
    }
    terminal.show();
    const exe = globwPath();
    const cmd = process.platform === 'win32' ? `& "${exe}" "${doc.fileName}"` : `"${exe}" "${doc.fileName}"`;
    terminal.sendText(cmd);
}

/** The word (possibly module-qualified, like `nungw.sqrt`) at a position. */
function wordAt(doc, pos) {
    const range = doc.getWordRangeAtPosition(pos, /[A-Za-z_][A-Za-z0-9_.]*/);
    return range ? doc.getText(range) : '';
}

const completionProvider = {
    provideCompletionItems() {
        const items = [];
        for (const [name, sig] of catalogue) {
            const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Function);
            item.detail = `${name}${sig}`;
            items.push(item);
        }
        for (const kw of KEYWORDS) {
            items.push(new vscode.CompletionItem(kw, vscode.CompletionItemKind.Keyword));
        }
        return items;
    },
};

/** This file's definitions (functions, classes, …) from `gw --symbols`. */
function getSymbols(doc) {
    return new Promise((resolve) => {
        if (doc.languageId !== 'globw' || doc.uri.scheme !== 'file') {
            resolve([]);
            return;
        }
        execFile(globwPath(), ['--symbols', doc.fileName], { windowsHide: true, maxBuffer: 8 * 1024 * 1024 }, (err, stdout) => {
            if (err && err.code === 'ENOENT') {
                resolve([]);
                return;
            }
            try {
                resolve(JSON.parse(stdout || '[]'));
            } catch {
                resolve([]);
            }
        });
    });
}

const SYMBOL_KIND = {
    class: vscode.SymbolKind.Class,
    method: vscode.SymbolKind.Method,
    field: vscode.SymbolKind.Field,
    function: vscode.SymbolKind.Function,
    procedure: vscode.SymbolKind.Function,
    variable: vscode.SymbolKind.Variable,
};

function symbolRange(s) {
    const line = Math.max(0, s.line - 1);
    const col = Math.max(0, s.col - 1);
    return new vscode.Range(line, col, line, col + (s.len || 1));
}

const documentSymbolProvider = {
    async provideDocumentSymbols(doc) {
        const syms = await getSymbols(doc);
        const top = [];
        const classes = new Map();
        for (const s of syms) {
            const range = symbolRange(s);
            const item = new vscode.DocumentSymbol(s.name, s.container || '', SYMBOL_KIND[s.kind] || vscode.SymbolKind.Variable, range, range);
            if (s.container && classes.has(s.container)) {
                classes.get(s.container).children.push(item);
            } else {
                top.push(item);
                if (s.kind === 'class') classes.set(s.name, item);
            }
        }
        return top;
    },
};

const definitionProvider = {
    async provideDefinition(doc, pos) {
        const range = doc.getWordRangeAtPosition(pos, /[A-Za-z_][A-Za-z0-9_]*/);
        if (!range) return null;
        const word = doc.getText(range);
        const syms = await getSymbols(doc);
        // prefer a top-level definition; otherwise the first match by name
        const hit = syms.find((s) => s.name === word && !s.container) || syms.find((s) => s.name === word);
        return hit ? new vscode.Location(doc.uri, symbolRange(hit)) : null;
    },
};

const formattingProvider = {
    provideDocumentFormattingEdits(doc) {
        return new Promise((resolve) => {
            const child = execFile(globwPath(), ['fmt'], { windowsHide: true, maxBuffer: 8 * 1024 * 1024 }, (err, stdout) => {
                if (err || !stdout) {
                    resolve([]); // gw missing, or an older gw without fmt: leave the file as is
                    return;
                }
                const full = new vscode.Range(doc.positionAt(0), doc.positionAt(doc.getText().length));
                resolve([vscode.TextEdit.replace(full, stdout)]);
            });
            child.stdin.end(doc.getText());
        });
    },
};

const hoverProvider = {
    provideHover(doc, pos) {
        const word = wordAt(doc, pos);
        const sig = catalogue.get(word);
        if (!sig) return undefined;
        const md = new vscode.MarkdownString();
        md.appendCodeblock(`${word}${sig}`, 'globw');
        return new vscode.Hover(md);
    },
};

function activate(context) {
    const diagnostics = vscode.languages.createDiagnosticCollection('globw');
    loadCatalogue();
    context.subscriptions.push(
        diagnostics,
        vscode.commands.registerCommand('globw.run', run),
        vscode.languages.registerCompletionItemProvider('globw', completionProvider, '.'),
        vscode.languages.registerHoverProvider('globw', hoverProvider),
        vscode.languages.registerDocumentFormattingEditProvider('globw', formattingProvider),
        vscode.languages.registerDocumentSymbolProvider('globw', documentSymbolProvider),
        vscode.languages.registerDefinitionProvider('globw', definitionProvider),
        vscode.workspace.onDidOpenTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidSaveTextDocument((doc) => check(doc, diagnostics)),
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('globw.path')) loadCatalogue();
        }),
        vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri))
    );
    vscode.workspace.textDocuments.forEach((doc) => check(doc, diagnostics));
}

function deactivate() {}

module.exports = { activate, deactivate };
