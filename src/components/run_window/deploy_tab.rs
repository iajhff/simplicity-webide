use leptos::{component, view, IntoView};

#[component]
pub fn DeployTab() -> impl IntoView {
    view! {
        <div class="tab-content deploy-tab">
            <p class="tab-description">
                "Deploy your Simplicity contract to Liquid testnet."
            </p>
            
            <div class="deploy-instructions">
                <h3>"Deployment Steps"</h3>
                
                <div class="deploy-step">
                    <h4>"1. Generate Address"</h4>
                    <p>"Click the 'Address' button in the toolbar to copy your contract address."</p>
                </div>
                
                <div class="deploy-step">
                    <h4>"2. Fund Your Address"</h4>
                    <p>"Visit the " <a href="https://liquidtestnet.com/faucet" target="_blank">"Liquid Testnet Faucet"</a> " and paste your address."</p>
                    <p>"The faucet will send 100,000 sats to your address."</p>
                </div>
                
                <div class="deploy-step">
                    <h4>"3. Enter Transaction Details"</h4>
                    <p>"Go to the 'Transaction' tab and enter the txid, vout, and value from the faucet."</p>
                    <p>"Or use the " <a href="https://blockstream.info/liquidtestnet/" target="_blank">"Blockstream Explorer"</a> " to look up your funding transaction."</p>
                </div>
                
                <div class="deploy-step">
                    <h4>"4. Generate Signatures"</h4>
                    <p>"Go to the 'Key Store' tab and click a signature button (e.g., 'Alice')."</p>
                    <p>"Paste the signature into your program's " <code>"mod witness"</code> " block."</p>
                </div>
                
                <div class="deploy-step">
                    <h4>"5. Generate Transaction"</h4>
                    <p>"Click the 'Transaction' button in the toolbar to generate your spending transaction."</p>
                </div>
                
                <div class="deploy-step">
                    <h4>"6. Broadcast"</h4>
                    <p>"Visit the " <a href="https://blockstream.info/liquidtestnet/tx/push" target="_blank">"Blockstream TX Push"</a> " page."</p>
                    <p>"Paste your transaction hex and click 'Broadcast transaction'."</p>
                </div>
            </div>
            
            <div class="deploy-note">
                <p><strong>"Note:"</strong>" All cryptographic operations happen locally in your browser using WASM. Your keys never leave your device."</p>
            </div>
        </div>
    }
}

