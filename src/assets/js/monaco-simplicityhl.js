/**
 * Monaco Editor Configuration for SimplicityHL
 * 
 * Provides syntax highlighting, autocompletion, and hover information
 * for the Simplicity smart contract language (Simfony).
 */

class MonacoSimplicityConfig {
    constructor() {
        this.editor = null;
        this.decorations = [];
        
        // Comprehensive Jet Definitions
        this.jets = [
            // Arithmetic
            { name: "add_8", type: "(Word8, Word8) -> (Bit, Word8)", description: "Addition of two 8-bit words returning carry bit and sum" },
            { name: "add_16", type: "(Word16, Word16) -> (Bit, Word16)", description: "Addition of two 16-bit words returning carry bit and sum" },
            { name: "add_32", type: "(Word32, Word32) -> (Bit, Word32)", description: "Addition of two 32-bit words returning carry bit and sum" },
            { name: "add_64", type: "(Word64, Word64) -> (Bit, Word64)", description: "Addition of two 64-bit words returning carry bit and sum" },
            { name: "subtract_32", type: "(Word32, Word32) -> (Bit, Word32)", description: "Subtraction of two 32-bit words returning borrow bit and difference" },
            { name: "subtract_64", type: "(Word64, Word64) -> (Bit, Word64)", description: "Subtraction of two 64-bit words returning borrow bit and difference" },
            { name: "multiply_32", type: "(Word32, Word32) -> Word64", description: "Multiply two 32-bit words producing 64-bit result" },
            { name: "multiply_64", type: "(Word64, Word64) -> Word128", description: "Multiply two 64-bit words producing 128-bit result" },
            { name: "div_mod_32", type: "(Word32, Word32) -> (Word32, Word32)", description: "Division and modulo of two 32-bit words" },
            { name: "div_mod_64", type: "(Word64, Word64) -> (Word64, Word64)", description: "Division and modulo of two 64-bit words" },
            
            // Bitwise
            { name: "and_32", type: "(Word32, Word32) -> Word32", description: "Bitwise AND of two 32-bit words" },
            { name: "or_32", type: "(Word32, Word32) -> Word32", description: "Bitwise OR of two 32-bit words" },
            { name: "xor_32", type: "(Word32, Word32) -> Word32", description: "Bitwise XOR of two 32-bit words" },
            { name: "complement_32", type: "Word32 -> Word32", description: "Bitwise NOT of 32-bit word" },
            { name: "left_shift_32", type: "(Word8, Word32) -> Word32", description: "Left shift 32-bit word by specified amount" },
            { name: "right_shift_32", type: "(Word8, Word32) -> Word32", description: "Right shift 32-bit word by specified amount" },
            
            // Comparison
            { name: "eq_32", type: "(Word32, Word32) -> Bit", description: "Test equality of two 32-bit words" },
            { name: "eq_256", type: "(Word256, Word256) -> Bit", description: "Test equality of two 256-bit words" },
            { name: "lt_32", type: "(Word32, Word32) -> Bit", description: "Test if first 32-bit word is less than second" },
            { name: "lt_64", type: "(Word64, Word64) -> Bit", description: "Test if first 64-bit word is less than second" },
            { name: "is_zero_32", type: "Word32 -> Bit", description: "Test if 32-bit word is zero" },
            
            // Crypto & Hashing
            { name: "sha_256", type: "Hash256 -> Hash256", description: "Compute SHA-256 hash" },
            { name: "sha_256_block", type: "(Hash256, Block512) -> Hash256", description: "Process single 512-bit block through SHA256" },
            { name: "sha_256_ctx_8_init", type: "() -> Ctx8", description: "Initialize SHA256 context" },
            { name: "sha_256_ctx_8_add_32", type: "(Ctx8, Word32) -> Ctx8", description: "Add 32-bit data to SHA256 context" },
            { name: "sha_256_ctx_8_finalize", type: "Ctx8 -> Hash256", description: "Finalize SHA256 hash computation" },
            { name: "bip_0340_verify", type: "((PubKey, Word256), Sig) -> ()", description: "Verify BIP-340 Schnorr signature" },
            { name: "check_sig_verify", type: "((PubKey, Word512), Sig) -> ()", description: "Verify Schnorr signature and fail if invalid" },
            { name: "fe_normalize", type: "FE -> FE", description: "Normalize secp256k1 field element" },
            
            // Elements / Liquid
            { name: "version", type: "() -> Word32", description: "Get transaction version number" },
            { name: "lock_time", type: "() -> Word32", description: "Get transaction lock time" },
            { name: "input_index", type: "() -> Word32", description: "Get index of current input" },
            { name: "input_value", type: "Word32 -> Maybe Word64", description: "Get value of specified input" },
            { name: "input_asset", type: "Word32 -> Maybe ConfWord256", description: "Get asset ID of specified input" },
            { name: "output_value", type: "Word32 -> Maybe Word64", description: "Get value of specified output" },
            { name: "output_asset", type: "Word32 -> Maybe ConfWord256", description: "Get asset ID of specified output" },
            { name: "current_index", type: "() -> Word32", description: "Get index of current input being validated" },
            { name: "current_value", type: "() -> Word64", description: "Get value of current input" },
            { name: "num_inputs", type: "() -> Word32", description: "Get number of transaction inputs" },
            { name: "num_outputs", type: "() -> Word32", description: "Get number of transaction outputs" },
            { name: "check_lock_height", type: "Word32 -> ()", description: "Verify input's locktime uses block height" },
            { name: "check_lock_time", type: "Word32 -> ()", description: "Verify input's locktime uses block time" },
            { name: "sig_all_hash", type: "() -> Word256", description: "Compute SIGHASH_ALL message hash" }
        ];
    }

    /**
     * Initialize Monaco Editor
     */
    async init(containerId = 'monaco-editor', initialCode = null) {
        // Load Monaco
        require.config({ 
            paths: { 
                vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' 
            }
        });

        return new Promise((resolve) => {
            require(['vs/editor/editor.main'], () => {
                // Register SimplicityHL language
                this.registerSimplicityHL();
                
                // Create editor
                this.editor = monaco.editor.create(
                    document.getElementById(containerId),
                    {
                        value: initialCode || this.getDefaultCode(),
                        language: 'simplicityhl',
                        theme: 'simplicity-dark',
                        minimap: { enabled: true },
                        fontSize: 14,
                        lineNumbers: 'on',
                        roundedSelection: true,
                        scrollBeyondLastLine: false,
                        readOnly: false,
                        automaticLayout: true,
                        folding: true,
                        renderWhitespace: 'selection',
                        suggestOnTriggerCharacters: true,
                        quickSuggestions: true,
                        wordBasedSuggestions: true
                    }
                );

                // Setup completion provider
                this.setupCompletionProvider();
                
                // Setup hover provider
                this.setupHoverProvider();

                resolve(this.editor);
            });
        });
    }

    /**
     * Register SimplicityHL language
     */
    registerSimplicityHL() {
        // Register language
        monaco.languages.register({ id: 'simplicityhl' });

        // Define tokens
        monaco.languages.setMonarchTokensProvider('simplicityhl', {
            keywords: [
                'fn', 'let', 'match', 'assert', 'witness', 'jet', 
                'if', 'else', 'Left', 'Right', 'true', 'false', 'return',
                'Some', 'None'
            ],
            typeKeywords: [
                'u8', 'u16', 'u32', 'u64', 'u128', 'u256',
                'Pubkey', 'Signature', 'Either', 'Option', 'bool',
                'Word8', 'Word16', 'Word32', 'Word64', 'Word256',
                'Ctx8', 'FE', 'GE', 'GEJ', 'Point', 'Scalar', 'Height', 'Time'
            ],
            operators: [
                '=', '==', '!=', '<', '>', '<=', '>=', 
                '+', '-', '*', '/', '!', '&&', '||', '=>', ':', ';'
            ],
            symbols: /[=><!~?:&|+\-*\/\^%]+/,
            
            tokenizer: {
                root: [
                    // Identifiers and keywords
                    [/[a-z_$][\w$]*/, {
                        cases: {
                            '@keywords': 'keyword',
                            '@typeKeywords': 'type',
                            '@default': 'identifier'
                        }
                    }],
                    
                    // Witness references
                    [/witness::[A-Z_]+/, 'variable.witness'],
                    
                    // Jet calls
                    [/jet::[a-z_0-9]+/, 'function.jet'],
                    
                    // Numbers
                    [/0x[0-9a-fA-F]+/, 'number.hex'],
                    [/\d+/, 'number'],
                    
                    // Strings
                    [/"([^"\\]|\\.)*$/, 'string.invalid'],
                    [/"/, 'string', '@string'],
                    
                    // Comments
                    [/\/\/.*$/, 'comment'],
                    [/\/\*/, 'comment', '@comment'],
                    
                    // Operators
                    [/@symbols/, {
                        cases: {
                            '@operators': 'operator',
                            '@default': ''
                        }
                    }],
                ],
                
                comment: [
                    [/[^\/*]+/, 'comment'],
                    [/\*\//, 'comment', '@pop'],
                    [/[\/*]/, 'comment']
                ],
                
                string: [
                    [/[^\\"]+/, 'string'],
                    [/\\./, 'string.escape'],
                    [/"/, 'string', '@pop']
                ]
            }
        });

        // Define theme
        monaco.editor.defineTheme('simplicity-dark', {
            base: 'vs-dark',
            inherit: true,
            rules: [
                { token: 'keyword', foreground: 'C586C0', fontStyle: 'bold' },
                { token: 'type', foreground: '4EC9B0', fontStyle: 'bold' },
                { token: 'function.jet', foreground: 'DCDCAA' },
                { token: 'variable.witness', foreground: '9CDCFE', fontStyle: 'italic' },
                { token: 'number', foreground: 'B5CEA8' },
                { token: 'number.hex', foreground: 'B5CEA8' },
                { token: 'string', foreground: 'CE9178' },
                { token: 'comment', foreground: '6A9955', fontStyle: 'italic' },
                { token: 'operator', foreground: 'D4D4D4' }
            ],
            colors: {
                'editor.background': '#1E1E1E',
                'editor.foreground': '#D4D4D4',
                'editor.lineHighlightBackground': '#2A2A2A',
                'editorCursor.foreground': '#AEAFAD',
                'editor.selectionBackground': '#264F78',
                'editor.inactiveSelectionBackground': '#3A3D41'
            }
        });

        // Set language configuration
        monaco.languages.setLanguageConfiguration('simplicityhl', {
            comments: {
                lineComment: '//',
                blockComment: ['/*', '*/']
            },
            brackets: [
                ['{', '}'],
                ['[', ']'],
                ['(', ')']
            ],
            autoClosingPairs: [
                { open: '{', close: '}' },
                { open: '[', close: ']' },
                { open: '(', close: ')' },
                { open: '"', close: '"' },
                { open: '\'', close: '\'' }
            ],
            surroundingPairs: [
                { open: '{', close: '}' },
                { open: '[', close: ']' },
                { open: '(', close: ')' },
                { open: '"', close: '"' },
                { open: '\'', close: '\'' }
            ],
            folding: {
                markers: {
                    start: new RegExp('^\\s*//\\s*#?region\\b'),
                    end: new RegExp('^\\s*//\\s*#?endregion\\b')
                }
            }
        });
    }

    /**
     * Setup autocomplete/IntelliSense
     */
    setupCompletionProvider() {
        monaco.languages.registerCompletionItemProvider('simplicityhl', {
            provideCompletionItems: (model, position) => {
                const suggestions = [];
                
                // Jet functions
                this.jets.forEach(jet => {
                    suggestions.push({
                        label: `jet::${jet.name}`,
                        kind: monaco.languages.CompletionItemKind.Function,
                        insertText: `jet::${jet.name}`,
                        detail: jet.type,
                        documentation: jet.description
                    });
                });
                
                // Keywords
                const keywords = [
                    'fn', 'let', 'match', 'assert', 'witness', 
                    'if', 'else', 'Left', 'Right', 'true', 'false', 'return'
                ];
                keywords.forEach(kw => {
                    suggestions.push({
                        label: kw,
                        kind: monaco.languages.CompletionItemKind.Keyword,
                        insertText: kw
                    });
                });
                
                // Types
                const types = [
                    'u8', 'u16', 'u32', 'u64', 'u128', 'u256',
                    'Pubkey', 'Signature', 'Either', 'Option', 'bool',
                    'Word8', 'Word16', 'Word32', 'Word64', 'Word256',
                    'Ctx8', 'FE', 'GE', 'GEJ', 'Point', 'Scalar', 'Height', 'Time'
                ];
                types.forEach(type => {
                    suggestions.push({
                        label: type,
                        kind: monaco.languages.CompletionItemKind.Class,
                        insertText: type
                    });
                });
                
                // Snippets
                suggestions.push({
                    label: 'fn main',
                    kind: monaco.languages.CompletionItemKind.Snippet,
                    insertText: 'fn main() {\n\t${1}\n}',
                    insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                    documentation: 'Main function template'
                });
                
                suggestions.push({
                    label: 'assert',
                    kind: monaco.languages.CompletionItemKind.Snippet,
                    insertText: 'assert!(${1:condition})',
                    insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                    documentation: 'Assertion statement'
                });
                
                suggestions.push({
                    label: 'witness',
                    kind: monaco.languages.CompletionItemKind.Snippet,
                    insertText: 'let ${1:name}: ${2:type} = witness::${3:NAME};',
                    insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
                    documentation: 'Witness declaration'
                });

                return { suggestions };
            }
        });
    }

    /**
     * Setup hover tooltips
     */
    setupHoverProvider() {
        monaco.languages.registerHoverProvider('simplicityhl', {
            provideHover: (model, position) => {
                const word = model.getWordAtPosition(position);
                if (!word) return null;

                const wordText = word.word;
                
                // Check if it's a jet
                const jet = this.jets.find(j => 
                    j.name === wordText || 
                    `jet::${j.name}` === wordText
                );

                if (jet) {
                    return {
                        contents: [
                            { value: `**jet::${jet.name}**` },
                            { value: `\`${jet.type}\`` },
                            { value: jet.description }
                        ]
                    };
                }

                return null;
            }
        });
    }

    /**
     * Get default code
     */
    getDefaultCode() {
        return `// Welcome to Simplicity IDE
// Write your Simplicity contract here

fn main() {
    // Example: Simple assertion
    assert!(true)
}`;
    }

    /**
     * Get editor value
     */
    getValue() {
        return this.editor ? this.editor.getValue() : '';
    }

    /**
     * Set editor value
     */
    setValue(code) {
        if (this.editor) {
            this.editor.setValue(code);
        }
    }

    /**
     * Format document
     */
    formatDocument() {
        if (this.editor) {
            this.editor.getAction('editor.action.formatDocument').run();
        }
    }
}

// Export for use in other scripts
if (typeof window !== 'undefined') {
    window.MonacoSimplicityConfig = MonacoSimplicityConfig;
}
if (typeof module !== 'undefined' && module.exports) {
    module.exports = MonacoSimplicityConfig;
}

