# Easier — Specifica del linguaggio e dell'interprete (v1)

Data: 2026-10-12

## 1. Obiettivo

Easier trasforma lo pseudocodice scolastico italiano in un linguaggio eseguibile.

- **Leggibile**: un esercizio scritto come a scuola deve girare così com'è.
- **Veloce per essere interpretato**: tipi dedotti staticamente, bytecode a registri specializzato per tipo.
- **Errori chiari in italiano**, trovati il più possibile *prima* dell'esecuzione.

File sorgente: estensione `.ez`. Eseguibile: `easier.exe`. Implementazione: Rust.

## 2. Struttura di un programma

Un programma è una sequenza di righe; **una istruzione per riga**. Ordine:

1. dichiarazioni globali (`GLOBAL elenco variabili: ...`), opzionali;
2. dichiarazioni del programma principale (`elenco variabili: ...`);
3. definizioni di `FUNZIONE` / `PROCEDURA` e istruzioni del programma principale, in qualsiasi ordine. Le istruzioni del principale vengono eseguite dall'alto in basso; le funzioni possono essere chiamate anche prima della loro definizione nel testo.

Commenti: da `'` a fine riga. Righe vuote ignorate.

```
GLOBAL elenco variabili: contatore
elenco variabili: n, voti[5]

FUNZIONE doppio(x)
    RESTITUISCI x * 2
FINE FUNZIONE

input(n)
output(doppio(n))
```

## 3. Lessico

- **Parole chiave solo in MAIUSCOLO**: `SE`, `ALTRIMENTI`, `FINE SE`, `FINCHE` (accettato anche `FINCHÉ`), `PER`, `DA`, `A`, `PASSO`, `RIPETI`, `FINE CICLO`, `FUNZIONE`, `FINE FUNZIONE`, `PROCEDURA`, `FINE PROCEDURA`, `RESTITUISCI`, `GLOBAL`, `E`, `O`, `NON`, `DIV`, `MOD`, `VERO`, `FALSO`.
  Sono riservate: nessuna variabile può chiamarsi così (quindi niente variabili `A`, `E`, `O`).
  Scritte in altro modo (`se`, `Se`) non sono parole chiave: vengono trattate come nomi, e l'errore che ne deriva suggerisce la forma maiuscola.
- **Dichiarazioni**: `elenco variabili:` si scrive in minuscolo, esattamente così.
- **Funzioni predefinite**: `input`, `output` (minuscole); `LUNGHEZZA`, `AGGIUNGI`, `RIMUOVI` (maiuscole).
- **Identificatori**: lettere (anche accentate), cifre, `_`; non iniziano con una cifra. Distinguono maiuscole e minuscole.
- **Letterali**: interi `42`, reali `3.14`, stringhe `"testo"` (escape `\"`, `\\`, `\n`), booleani `VERO` / `FALSO`.
- **Assegnamento**: `<-` oppure `←`.

## 4. Variabili e tipi

### 4.1 Dichiarazione

```
elenco variabili: a, b, voti[10], matrice[3, 4], nomi[]
GLOBAL elenco variabili: totale
```

- Ogni variabile usata deve comparire in un `elenco variabili`. Si possono avere più righe `elenco variabili:` nello stesso ambito, tutte in testa all'ambito.
- `nome[N]` = array di N elementi (N letterale intero > 0). `nome[N, M]` = matrice N×M.
- `nome[]` = lista dinamica, inizialmente vuota.
- `GLOBAL elenco variabili:` (solo all'inizio del programma) rende quelle variabili visibili ovunque, funzioni comprese. Le variabili di `elenco variabili:` del principale **non** sono visibili dentro le funzioni.
- Dentro una funzione/procedura, `elenco variabili:` dichiara variabili locali. Un locale con lo stesso nome di un globale lo nasconde.

### 4.2 Tipi

`intero` (64 bit), `reale` (64 bit), `stringa`, `booleano`, `array di T`, `matrice di T`, `lista di T`.

Il tipo **non si scrive**: l'interprete lo deduce da tutti gli usi della variabile nel programma (assegnamenti, operazioni, confronti, argomenti). Regole:

- Una variabile ha un solo tipo per tutto il programma. Usi incompatibili sono un errore prima dell'esecuzione:
  `Errore alla riga 9: "x" è un intero (riga 3), qui le assegni una stringa`.
- Se una variabile riceve sia interi sia reali, è `reale` (l'intero viene convertito).
- Una variabile usata solo con `input` e mai altrove con un tipo deducibile è `stringa`.
- Gli elementi di array, matrici e liste hanno tutti lo stesso tipo, dedotto allo stesso modo.

### 4.3 Valore prima dell'uso

- Leggere una variabile semplice che, in qualche percorso del programma, potrebbe non aver ancora ricevuto un valore è un **errore prima dell'esecuzione** (analisi di assegnamento definito, come in Java/C#).
- Leggere un elemento di array/matrice mai assegnato è un **errore durante l'esecuzione**.

## 5. Espressioni

Precedenza, dalla più alta alla più bassa:

| Livello | Operatori |
|---|---|
| 1 | chiamate `f(...)`, indici `v[i]`, `m[i, j]` |
| 2 | `-` unario, `NON` |
| 3 | `*`, `/`, `DIV`, `MOD` |
| 4 | `+`, `-` |
| 5 | `=`, `!=`, `<`, `<=`, `>`, `>=` (non concatenabili: `a < b < c` è errore) |
| 6 | `E` |
| 7 | `O` |

- `/` produce sempre un `reale`. `DIV` e `MOD` solo su interi.
- `+` tra due stringhe concatena; `stringa + numero/booleano` (o viceversa) converte l'altro operando in testo e concatena.
- `=` e `!=` confrontano valori dello stesso tipo (intero e reale si confrontano tra loro). `<` ecc. su numeri e stringhe (ordine alfabetico).
- `E`, `O` valutano in corto circuito.
- Overflow di interi e divisione per zero: errore durante l'esecuzione.
- Parentesi `( )` per raggruppare.

**Ambiguità della freccia**: `<-` è sempre un assegnamento, tranne dentro un'espressione, dove `x<-1` significa `x < -1`.

## 6. Istruzioni

### 6.1 Assegnamento
```
x <- 5
voti[i] <- 7
m[i, j] <- 0
```

### 6.2 Condizione
```
SE(cond)
    ...
FINE SE

SE(cond)
    ...
ALTRIMENTI
    ...
FINE SE

SE(voto >= 8)
    output("ottimo")
ALTRIMENTI
SE(voto >= 6)
    output("sufficiente")
ALTRIMENTI
    output("insufficiente")
FINE SE
```
Un `ALTRIMENTI` seguito subito (riga successiva, ignorando righe vuote e commenti) da `SE(...)` forma **sempre** un "altrimenti se": la catena si chiude con **un solo** `FINE SE`. Un `FINE SE` in più dà un errore che spiega questa regola. Per un `SE` annidato vero dentro un `ALTRIMENTI`, basta che non sia la prima istruzione del blocco. La condizione deve essere `booleano`.

### 6.3 Cicli
```
FINCHE(cond)                      ' ripete finché cond è VERO
    ...
FINE CICLO

PER(i DA inizio A fine)           ' estremi inclusi, passo 1
    ...
FINE CICLO

PER(i DA 10 A 0 PASSO -2)
    ...
FINE CICLO

RIPETI                            ' esegue almeno una volta,
    ...                           ' poi ripete finché cond è VERO
FINCHE(cond)
```
- In `PER`: `i` deve essere dichiarata ed è `intero`; `inizio`, `fine`, `PASSO` sono interi, valutati una volta sola all'ingresso. `PASSO 0` è errore durante l'esecuzione. Con passo positivo si cicla finché `i <= fine`, con passo negativo finché `i >= fine`.
- `FINE CICLO` chiude `FINCHE` e `PER`. `RIPETI` si chiude con `FINCHE(cond)`.

### 6.4 Funzioni e procedure
```
FUNZIONE nome(p1, p2)
    elenco variabili: locale
    ...
    RESTITUISCI espressione
FINE FUNZIONE

PROCEDURA nome(p1)
    ...
FINE PROCEDURA
```
- Si definiscono solo al livello principale (non annidate). Ricorsione permessa.
- `FUNZIONE` deve eseguire `RESTITUISCI valore` su ogni percorso (controllato prima dell'esecuzione). `PROCEDURA` può usare `RESTITUISCI` senza valore per uscire prima; non può essere usata dentro un'espressione.
- I parametri sono variabili locali, non vanno in `elenco variabili`.
- Tipi dei parametri dedotti a ogni chiamata: se una funzione viene chiamata con tipi diversi (es. `doppio(3)` e `doppio(2.5)`), l'interprete ne genera una versione specializzata per ciascuna combinazione. Il tipo restituito si deduce dai `RESTITUISCI`.
- Passaggio: valori semplici per copia; array, matrici e liste per riferimento (le modifiche si vedono fuori).

### 6.5 Input e output
```
input(a)          ' legge una riga e la converte nel tipo di a
input(a, b)       ' legge una riga per ogni variabile
output("Somma: ", s)
```
- `output` scrive gli argomenti uno dopo l'altro, senza separatori, e va a capo.
- Reali stampati nella forma più corta che li rappresenta, sempre con un punto decimale (`2.5`, `3.0`). Booleani stampati come `VERO` / `FALSO`.
- `input` per un reale accetta sia `3.5` sia `3,5`. Testo non convertibile nel tipo richiesto: errore durante l'esecuzione che indica cosa si aspettava. `input` può leggere anche un elemento: `input(voti[i])`.

### 6.6 Funzioni predefinite
| Nome | Effetto |
|---|---|
| `LUNGHEZZA(x)` | numero di elementi di array/lista, o di caratteri di una stringa |
| `AGGIUNGI(lista, valore)` | aggiunge in fondo alla lista (procedura) |
| `RIMUOVI(lista, posizione)` | toglie l'elemento in quella posizione (da 1) (procedura) |

Gli indici partono da **1** per array, matrici, liste e stringhe. Indice fuori dai limiti: errore durante l'esecuzione.

## 7. Errori

Formato unico, in italiano, con riga, colonna e indicazione del punto:
```
Errore alla riga 7, colonna 5: la variabile "totale" non è nell'elenco variabili
    totale <- 0
    ^^^^^^
Suggerimento: aggiungila a "elenco variabili:" in cima al programma
```
- **Prima dell'esecuzione** (sintassi, nomi, tipi, valore prima dell'uso, `RESTITUISCI` mancante): vengono riportati tutti gli errori trovati, fino a un massimo di 20; il programma non parte.
- **Durante l'esecuzione** (indice fuori dai limiti, divisione per zero, overflow, input non valido, elemento non assegnato, `PASSO 0`): il programma si ferma con lo stesso formato, indicando la riga in esecuzione.
- Codici di uscita: 0 = ok, 1 = errore prima dell'esecuzione, 2 = errore durante l'esecuzione, 3 = errore d'uso (file mancante ecc.).

## 8. Architettura

```
sorgente.ez → Lexer → Parser → Analisi → Compilatore → VM
               token    AST    nomi+tipi    bytecode
```

Progetto Cargo `easier/`, un modulo per fase:

| Modulo | Compito | Dipende da |
|---|---|---|
| `errori.rs` | tipo `Errore` (posizione, messaggio, suggerimento) e stampa formattata | — |
| `lexer.rs` | testo → token con riga/colonna | errori |
| `ast.rs` | strutture dell'albero | — |
| `parser.rs` | token → AST; controllo dei blocchi (`FINE SE`, `FINE CICLO`…) | lexer, ast, errori |
| `analisi.rs` | risoluzione nomi e ambiti, deduzione tipi (unificazione), specializzazione funzioni, assegnamento definito, percorsi di `RESTITUISCI` → AST tipizzato | ast, errori |
| `bytecode.rs` | definizione delle istruzioni | — |
| `compilatore.rs` | AST tipizzato → bytecode a registri | analisi, bytecode |
| `vm.rs` | esecuzione del bytecode | bytecode, errori |
| `main.rs` | riga di comando | tutti |

**Scelte per la velocità**
- Bytecode **a registri** (come Lua 5): meno istruzioni rispetto a uno stack.
- **Istruzioni specializzate per tipo** (`SOMMA_INT`, `SOMMA_REALE`, `CONFRONTA_INT`…): grazie ai tipi già noti, la VM non controlla i tipi a ogni operazione.
- Interi e reali senza allocazioni; stringhe, array e liste condivisi per riferimento.
- Variabili globali in slot a indice fisso, mai cercate per nome durante l'esecuzione.

**Riga di comando**
```
easier programma.ez              esegue
easier --controlla programma.ez  solo controllo errori, non esegue
```

## 9. Test

- **Unitari** per ogni modulo (lexer, parser, analisi, compilatore, vm).
- **Esempi**: cartella `esempi/` con coppie `nome.ez` + `nome.atteso` (output atteso) e, dove serve, `nome.input`. Un test li esegue tutti e confronta l'output. Include anche esempi che *devono* fallire, con il messaggio d'errore atteso.
- **Benchmark**: Fibonacci ricorsivo (fib 30), crivello di Eratostene (fino a 10 milioni), ciclo di somme (100 milioni di iterazioni), confrontati con gli stessi programmi in Python.
  **Obiettivo**: almeno 5 volte più veloce di CPython su tutti e tre.

## 10. Fuori dalla v1

Lettura/scrittura di file, moduli e import, record/strutture, funzioni matematiche e casuali, evidenziazione della sintassi per editor, REPL interattivo, compilazione JIT.
