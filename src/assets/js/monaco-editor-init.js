/**
 * Monaco Editor Integration for Simplicity Web IDE
 * Uses Monaco Editor (VS Code's editor) instead of CodeMirror
 */

(function() {
    'use strict';
    
    // Initialize as soon as DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initializeMonaco);
    } else {
        initializeMonaco();
    }
    
    function initializeMonaco() {
        const textarea = document.querySelector('textarea[name="program-input"]');
        
        if (!textarea) {
            setTimeout(initializeMonaco, 50);
            return;
        }
        
        if (!window.require) {
            console.error('Monaco: RequireJS not loaded');
            textarea.style.display = '';
            return;
        }
        
        try {
            console.log('Monaco: Initializing...');
            
            // Create a container for Monaco
            const container = document.createElement('div');
            container.id = 'monaco-editor-container';
            container.style.height = '600px';
            container.style.border = '1px solid rgba(255, 255, 255, 0.10)';
            container.style.borderRadius = '7.5px';
            
            // Hide textarea and insert Monaco container
            textarea.style.display = 'none';
            textarea.parentNode.insertBefore(container, textarea.nextSibling);
            
            // Configure Monaco loader
            require.config({ 
                paths: { 
                    vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' 
                }
            });
            
            // Load Monaco and initialize
            require(['vs/editor/editor.main'], function() {
                // Initialize Simplicity language support
                if (window.MonacoSimplicityConfig) {
                    const simplicityConfig = new window.MonacoSimplicityConfig();
                    
                    simplicityConfig.init('monaco-editor-container', textarea.value).then(function(editor) {
                        console.log('Monaco: Initialized successfully! ✨');
                        
                        // Track if we're updating from Monaco to prevent loops
                        let updatingFromMonaco = false;
                        let lastTextareaValue = textarea.value;
                        
                        // Sync changes back to textarea for Leptos
                        editor.onDidChangeModelContent(function() {
                            updatingFromMonaco = true;
                            textarea.value = editor.getValue();
                            lastTextareaValue = textarea.value;
                            const event = new Event('input', { bubbles: true });
                            textarea.dispatchEvent(event);
                            updatingFromMonaco = false;
                        });
                        
                        // Watch for external changes to textarea (like from Examples dropdown)
                        // Check periodically for textarea value changes from Leptos
                        setInterval(function() {
                            if (!updatingFromMonaco && textarea.value !== lastTextareaValue) {
                                lastTextareaValue = textarea.value;
                                editor.setValue(textarea.value);
                                console.log('Monaco: Updated from external source (Examples dropdown)');
                            }
                        }, 100);
                        
                        // Store editor reference globally for external updates
                        window.monacoEditor = editor;
                        window.updateMonacoEditor = function(newValue) {
                            if (editor && newValue !== editor.getValue()) {
                                lastTextareaValue = newValue;
                                editor.setValue(newValue);
                            }
                        };
                        
                        // Handle Ctrl+Enter
                        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, function() {
                            const runButton = document.querySelector('button.run-button');
                            if (runButton) {
                                runButton.click();
                            }
                        });
                        
                        // Focus editor
                        editor.focus();
                    }).catch(function(error) {
                        console.error('Monaco: Initialization failed:', error);
                        textarea.style.display = '';
                        container.style.display = 'none';
                    });
                } else {
                    console.error('Monaco: MonacoSimplicityConfig not found');
                    textarea.style.display = '';
                    container.style.display = 'none';
                }
            });
            
        } catch (error) {
            console.error('Monaco: Failed to initialize:', error);
            textarea.style.display = '';
        }
    }
})();

