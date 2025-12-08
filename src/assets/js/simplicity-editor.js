/**
 * Simplicity Editor Enhancement
 * Pure client-side CodeMirror integration - no Rust changes needed
 */

(function() {
    'use strict';
    
    // Initialize as soon as DOM is ready (faster than 'load')
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initializeCodeMirror);
    } else {
        // DOM already loaded
        initializeCodeMirror();
    }
    
    function initializeCodeMirror() {
        // Find the program input textarea
        const textarea = document.querySelector('textarea[name="program-input"]');
        
        if (!textarea) {
            // Retry after a short delay if textarea isn't ready yet
            setTimeout(initializeCodeMirror, 50);
            return;
        }
        
        if (!window.CodeMirror) {
            console.error('Simplicity: CodeMirror not loaded');
            // Show the textarea as fallback
            textarea.style.display = '';
            return;
        }
        
        try {
            console.log('Simplicity: Initializing CodeMirror...');
            
            // Create CodeMirror instance
            const editor = CodeMirror.fromTextArea(textarea, {
                mode: 'simplicityhl',
                theme: 'simplicity',
                lineNumbers: true,
                matchBrackets: true,
                autoCloseBrackets: true,
                indentUnit: 4,
                tabSize: 4,
                indentWithTabs: false,
                lineWrapping: false,
                extraKeys: {
                    "Tab": function(cm) {
                        cm.replaceSelection("    ", "end");
                    },
                    "Shift-Tab": function(cm) {
                        const cursor = cm.getCursor();
                        const line = cm.getLine(cursor.line);
                        if (line.startsWith("    ")) {
                            cm.replaceRange("", 
                                { line: cursor.line, ch: 0 },
                                { line: cursor.line, ch: 4 }
                            );
                        }
                    },
                    "Ctrl-Enter": function(cm) {
                        // Trigger the run button
                        const runButton = document.querySelector('button.run-button');
                        if (runButton) {
                            runButton.click();
                        }
                    }
                }
            });
            
            // Sync changes back to the textarea so Leptos sees them
            editor.on('change', function(cm) {
                textarea.value = cm.getValue();
                // Trigger input event so Leptos reactive system picks it up
                const event = new Event('input', { bubbles: true });
                textarea.dispatchEvent(event);
            });
            
            // Refresh editor immediately and after a moment to ensure proper sizing
            editor.refresh();
            setTimeout(function() {
                editor.refresh();
            }, 50);
            
            console.log('Simplicity: CodeMirror initialized successfully! ✨');
            
        } catch (error) {
            console.error('Simplicity: Failed to initialize CodeMirror:', error);
            // Show textarea as fallback
            textarea.style.display = '';
        }
    }
})();

