use leptos::{component, view, IntoView};

use crate::components::program_window::testnet_automation_button::TestnetAutomationButtons;

#[component]
pub fn DeployTab() -> impl IntoView {
    view! {
        <div class="tab-content deploy-tab">
            <p class="tab-description">
                "Deploy your Simplicity contract to Liquid testnet using automated workflow."
            </p>
            
            <TestnetAutomationButtons />
            
            <div class="deploy-note" style="margin-top: 20px;">
                <p><strong>"Note:"</strong>" All cryptographic operations happen locally in your browser using WASM. Your keys never leave your device."</p>
                <p>"This workflow uses the Blockstream Esplora API and Liquid Testnet Faucet for convenience."</p>
            </div>
        </div>
    }
}

