/**
 * Testnet Automation API
 * Handles automatic funding, UTXO lookup, and transaction broadcasting
 */

class TestnetAutomation {
    constructor() {
        this.FAUCET_API = 'https://liquidtestnet.com/faucet';
        this.ESPLORA_API = 'https://blockstream.info/liquidtestnet/api';
    }

    /**
     * Fund an address from the Liquid testnet faucet using CORS proxy
     * @param {string} address - The address to fund
     * @returns {Promise<{txid: string}>}
     */
    async fundFromFaucet(address) {
        const faucetUrl = `https://liquidtestnet.com/faucet?address=${encodeURIComponent(address)}&action=lbtc`;
        
        // Use CORS proxy
        const proxyUrl = `https://corsproxy.io/?${encodeURIComponent(faucetUrl)}`;
        
        try {
            const response = await fetch(proxyUrl);
            const html = await response.text();
            
            // Extract txid from HTML response
            const txidMatch = html.match(/([a-f0-9]{64})/);
            if (txidMatch) {
                return { txid: txidMatch[1] };
            }
            
            // Check for rate limiting
            if (html.toLowerCase().includes('limit') || html.toLowerCase().includes('wait')) {
                throw new Error('Rate limited - please wait before requesting again');
            }
            
            throw new Error('Could not extract txid from faucet response');
        } catch (error) {
            console.error('Faucet error:', error);
            throw error;
        }
    }

    /**
     * Lookup UTXO details for a transaction
     * @param {string} txid - Transaction ID
     * @param {string} address - Address to find in outputs
     * @param {number} maxRetries - Maximum number of retry attempts
     * @returns {Promise<{vout: number, value: number, txid: string}>}
     */
    async lookupUTXO(txid, address, maxRetries = 6) {
        let lastError = null;
        
        for (let attempt = 0; attempt < maxRetries; attempt++) {
            try {
                const response = await fetch(`${this.ESPLORA_API}/tx/${txid}`);
                
                if (!response.ok) {
                    if (response.status === 404) {
                        // Transaction not found yet, wait and retry
                        if (attempt < maxRetries - 1) {
                            console.log(`Transaction not found yet, waiting... (attempt ${attempt + 1}/${maxRetries})`);
                            await new Promise(resolve => setTimeout(resolve, 3000));
                            continue;
                        }
                        throw new Error(`Transaction not found after ${maxRetries} attempts. Check the txid or wait longer.`);
                    }
                    throw new Error(`API returned status ${response.status}`);
                }

                const tx = await response.json();
                
                console.log('Transaction found:', tx);
                console.log('Looking for address:', address);
                console.log('Available outputs:', tx.vout.map(o => o.scriptpubkey_address));
                
                // Find the output that matches our address
                const outputIndex = tx.vout.findIndex(output => {
                    return output.scriptpubkey_address === address;
                });

                if (outputIndex === -1) {
                    throw new Error(`Address not found in outputs. Expected: ${address}. Found: ${tx.vout.map(o => o.scriptpubkey_address).join(', ')}`);
                }

                const output = tx.vout[outputIndex];
                
                return {
                    txid: txid,
                    vout: outputIndex,
                    value: output.value
                };
            } catch (error) {
                lastError = error;
                
                // If it's not a "not found" error, don't retry
                if (!error.message.includes('not found') && !error.message.includes('404')) {
                    throw error;
                }
                
                // Wait before retry (except on last attempt)
                if (attempt < maxRetries - 1) {
                    await new Promise(resolve => setTimeout(resolve, 3000));
                }
            }
        }
        
        console.error('UTXO lookup error after retries:', lastError);
        throw new Error(`Lookup failed: ${lastError.message}`);
    }

    /**
     * Broadcast a raw transaction to the network
     * @param {string} rawTx - Hex-encoded raw transaction
     * @returns {Promise<{txid: string, status: string}>}
     */
    async broadcastTransaction(rawTx) {
        try {
            const response = await fetch(`${this.ESPLORA_API}/tx`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'text/plain',
                },
                body: rawTx
            });

            if (!response.ok) {
                const errorText = await response.text();
                throw new Error(this.parseErrorMessage(errorText));
            }

            const txid = await response.text();
            
            return {
                txid: txid.trim(),
                status: 'success',
                explorerUrl: `https://blockstream.info/liquidtestnet/tx/${txid.trim()}`,
                mempoolUrl: `https://liquid.network/testnet/tx/${txid.trim()}`
            };
        } catch (error) {
            console.error('Broadcast error:', error);
            throw error;
        }
    }

    /**
     * Parse error messages from Esplora API
     */
    parseErrorMessage(errorText) {
        const errorMap = {
            'Transaction already in block chain': 'UTXO already spent! Go back to Step 2 (Lookup UTXO) and click "Fund & Lookup" to get a fresh UTXO.',
            'bad-txns-inputs-missingorspent': 'UTXO doesn\'t exist or already spent. Run Step 2 (Lookup UTXO) again to get a fresh UTXO.',
            'bad-txns-in-ne-out': 'Input value ≠ output value. Check UTXO info (vout and value) and fee.',
            'bad-txns-fee-outofrange': 'Fee doesn\'t cover transaction weight. Increase the fee.',
            'non-final': 'Lock time is higher than current block height. Decrease locktime or wait.',
            'non-BIP68-final': 'Sequence is invalid for current block height. Decrease sequence or wait.',
            'dust': 'Creating dust output. Fee consumes entire input. Decrease the fee.',
            'non-mandatory-script-verify-flag (Assertion failed inside jet)': 'A Simplicity jet failed. Check your program conditions and witness data.',
            'non-mandatory-script-verify-flag (Witness program hash mismatch)': 'CMR mismatch between UTXO and transaction input. Use the original program.'
        };

        for (const [key, message] of Object.entries(errorMap)) {
            if (errorText.includes(key)) {
                return message;
            }
        }

        return errorText || 'Unknown broadcast error';
    }

    /**
     * Check if transaction is confirmed (in a block)
     * @param {string} txid - Transaction ID to check
     * @returns {Promise<boolean>}
     */
    async checkTransactionConfirmation(txid) {
        try {
            const response = await fetch(`${this.ESPLORA_API}/tx/${txid}/status`);
            if (!response.ok) {
                return false;
            }
            
            const status = await response.json();
            // Transaction is confirmed if it has a block_height
            return status.confirmed === true || (status.block_height && status.block_height > 0);
        } catch (error) {
            console.error('Confirmation check error:', error);
            return false;
        }
    }

    /**
     * Wait for transaction confirmation
     * @param {string} txid - Transaction ID to wait for
     * @param {number} maxAttempts - Maximum number of attempts
     * @returns {Promise<boolean>}
     */
    async waitForConfirmation(txid, maxAttempts = 12) {
        for (let i = 0; i < maxAttempts; i++) {
            try {
                const response = await fetch(`${this.ESPLORA_API}/tx/${txid}`);
                if (response.ok) {
                    return true;
                }
            } catch (error) {
                // Continue waiting
            }
            
            // Wait 5 seconds between attempts
            await new Promise(resolve => setTimeout(resolve, 5000));
        }
        
        return false;
    }
}

// Export for use in Leptos/WASM
if (typeof window !== 'undefined') {
    window.TestnetAutomation = TestnetAutomation;
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = TestnetAutomation;
}

