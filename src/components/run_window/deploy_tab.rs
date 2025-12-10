use leptos::{component, view, IntoView};

use crate::components::program_window::testnet_automation_button::TestnetAutomationButtons;

#[component]
pub fn DeployTab() -> impl IntoView {
    view! {
        <div class="tab deploy-tab">
            <TestnetAutomationButtons />
        </div>
    }
}

