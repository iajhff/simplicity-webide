use leptos::{component, create_rw_signal, create_signal, spawn_local, use_context, view, with, IntoView, SignalGet, SignalSet, SignalUpdate, SignalWith};
use web_sys::js_sys;

use crate::components::program_window::Program;
use crate::components::run_window::TxEnv;
use crate::util;

#[component]
pub fn TestnetAutomationButtons() -> impl IntoView {
    let program = use_context::<Program>().expect("program should exist in context");
    let tx_env = use_context::<TxEnv>().expect("transaction environment should exist in context");
    
    let (fund_status, set_fund_status) = create_signal(String::new());
    let (fund_loading, set_fund_loading) = create_signal(false);
    let (lookup_status, set_lookup_status) = create_signal(String::new());
    let (lookup_loading, set_lookup_loading) = create_signal(false);
    let (broadcast_status, set_broadcast_status) = create_signal(String::new());
    let (broadcast_loading, set_broadcast_loading) = create_signal(false);
    
    let funding_txid = create_rw_signal(String::new());
    
    // One-click auto-fund
    let auto_fund = move |_| {
        let address = program
            .cmr()
            .ok()
            .map(util::liquid_testnet_address)
            .map(|addr| addr.to_string())
            .unwrap_or_default();

        if address.is_empty() || address == "Invalid program" {
            set_fund_status.set("Invalid program - cannot generate address".to_string());
            return;
        }

        set_fund_loading.set(true);
        set_fund_status.set("Requesting funds from faucet...".to_string());

        spawn_local(async move {
            match call_fund_from_faucet(&address).await {
                Ok(txid) => {
                    funding_txid.set(txid.clone());
                    set_fund_status.set(format!("Funded! Txid: {}...", &txid[..16]));
                    set_fund_loading.set(false);
                }
                Err(err) => {
                    set_fund_status.set(format!("{}", err));
                    set_fund_loading.set(false);
                }
            }
        });
    };

    let lookup_utxo = move |_| {
        let txid = funding_txid.get();
        let address = program
            .cmr()
            .ok()
            .map(util::liquid_testnet_address)
            .map(|addr| addr.to_string())
            .unwrap_or_default();

        if txid.is_empty() {
            set_lookup_status.set("No funding transaction. Fund first.".to_string());
            return;
        }

        if address.is_empty() {
            set_lookup_status.set("Invalid program address".to_string());
            return;
        }

        set_lookup_loading.set(true);
        set_lookup_status.set("Looking up UTXO...".to_string());

        spawn_local(async move {
            match call_lookup_utxo(&txid, &address).await {
                Ok((vout, value)) => {
                    tx_env.params.update(|params| {
                        if let Ok(parsed_txid) = txid.parse() {
                            params.txid = parsed_txid;
                        }
                        params.vout = vout;
                        params.value_in = value;
                    });
                    set_lookup_status.set(format!("Found! vout={}, value={} sats", vout, value));
                    set_lookup_loading.set(false);
                }
                Err(err) => {
                    set_lookup_status.set(format!("Error: {}", err));
                    set_lookup_loading.set(false);
                }
            }
        });
    };

    // Broadcast transaction with helpful error messages
    let broadcast_tx = move |_| {
        set_broadcast_loading.set(true);
        set_broadcast_status.set("Generating transaction...".to_string());
        
        let params = tx_env.params;
        let env = tx_env.lazy_env;
        
        let raw_tx = with!(|params, env| {
            use elements::pset::serialize::Serialize;
            use hex_conservative::DisplayHex;
            use simplicityhl::elements;
            
            let satisfied = match program.satisfied() {
                Ok(x) => x,
                Err(e) => {
                    set_broadcast_status.set(format!("✗ Program error: {}. Fix your program first.", e));
                    set_broadcast_loading.set(false);
                    return String::new();
                }
            };
            let pruned = match satisfied.redeem().prune(env) {
                Ok(x) => x,
                Err(_e) => {
                    set_broadcast_status.set("✗ Missing signature! Go to Key Store tab → Click 'Alice' to copy signature → Paste into your program's witness section → Try again.".to_string());
                    set_broadcast_loading.set(false);
                    return String::new();
                }
            };
            params.transaction(&pruned).serialize().to_lower_hex_string()
        });
        
        if raw_tx.is_empty() {
            return;
        }

        set_broadcast_status.set("Broadcasting to network...".to_string());

        spawn_local(async move {
            match call_broadcast_transaction(&raw_tx).await {
                Ok((txid, explorer_url)) => {
                    set_broadcast_status.set(format!("✓ Success! Txid: {}...", &txid[..16]));
                    set_broadcast_loading.set(false);
                    
                    if let Some(window) = web_sys::window() {
                        let _ = window.open_with_url_and_target(&explorer_url, "_blank");
                    }
                }
                Err(err) => {
                    set_broadcast_status.set(format!("✗ Broadcast failed: {}", err));
                    set_broadcast_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="unified-workflow">
            <h3 class="workflow-title">"Complete Transaction Workflow"</h3>
            
            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"1"</span>
                    <h4>"Get Testnet Funds"</h4>
                </div>
                <button
                    class="workflow-button"
                    on:click=auto_fund
                    disabled=move || fund_loading.get()
                >
                    {move || if fund_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Requesting..."</> }
                    } else if !fund_status.get().is_empty() && fund_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" Funded"</> }
                    } else {
                        view! { <><i class="fas fa-faucet"></i>" Request Testnet L-BTC"</> }
                    }}
                </button>
                {move || {
                    let status = fund_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"2"</span>
                    <h4>"Lookup UTXO"</h4>
                </div>
                <button
                    class="workflow-button"
                    on:click=lookup_utxo
                    disabled=move || lookup_loading.get() || funding_txid.get().is_empty()
                >
                    {move || if lookup_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Looking up..."</> }
                    } else if !lookup_status.get().is_empty() && lookup_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" UTXO Found"</> }
                    } else {
                        view! { <><i class="fas fa-search"></i>" Find UTXO"</> }
                    }}
                </button>
                {move || {
                    let status = lookup_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"3"</span>
                    <h4>"Generate Signature"</h4>
                </div>
                <div class="manual-instruction">
                    <p>"Go to "<strong>"Key Store"</strong>" tab below → Click "<strong>"Alice"</strong>" button → Paste signature into your program"</p>
                </div>
            </div>

            <div class="workflow-step">
                <div class="step-header">
                    <span class="step-number">"4"</span>
                    <h4>"Broadcast Transaction"</h4>
                </div>
                <button
                    class="workflow-button primary"
                    on:click=broadcast_tx
                    disabled=move || broadcast_loading.get()
                >
                    {move || if broadcast_loading.get() {
                        view! { <><i class="fas fa-spinner fa-spin"></i>" Broadcasting..."</> }
                    } else if !broadcast_status.get().is_empty() && broadcast_status.get().contains("✓") {
                        view! { <><i class="fas fa-check-circle"></i>" Transaction Sent!"</> }
                    } else {
                        view! { <><i class="fas fa-rocket"></i>" Broadcast to Network"</> }
                    }}
                </button>
                {move || {
                    let status = broadcast_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
            </div>
        </div>
    }
}

async fn call_fund_from_faucet(address: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let fund_fn = js_sys::Reflect::get(&automation, &"fundFromFaucet".into())
        .map_err(|_| "fundFromFaucet method not found")?;
    
    let promise = js_sys::Reflect::apply(
        &fund_fn.into(),
        &automation,
        &js_sys::Array::of1(&address.into()),
    )
    .map_err(|_| "Failed to call fundFromFaucet")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let txid = js_sys::Reflect::get(&result, &"txid".into())
        .map_err(|_| "No txid in result")?
        .as_string()
        .ok_or("txid is not a string")?;
    
    Ok(txid)
}

async fn call_lookup_utxo(txid: &str, address: &str) -> Result<(u32, u64), String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let lookup_fn = js_sys::Reflect::get(&automation, &"lookupUTXO".into())
        .map_err(|_| "lookupUTXO method not found")?;
    
    let args = js_sys::Array::new();
    args.push(&txid.into());
    args.push(&address.into());
    
    let promise = js_sys::Reflect::apply(&lookup_fn.into(), &automation, &args)
        .map_err(|_| "Failed to call lookupUTXO")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let vout = js_sys::Reflect::get(&result, &"vout".into())
        .map_err(|_| "No vout in result")?
        .as_f64()
        .ok_or("vout is not a number")? as u32;
    
    let value = js_sys::Reflect::get(&result, &"value".into())
        .map_err(|_| "No value in result")?
        .as_f64()
        .ok_or("value is not a number")? as u64;
    
    Ok((vout, value))
}

async fn call_broadcast_transaction(raw_tx: &str) -> Result<(String, String), String> {
    let window = web_sys::window().ok_or("No window")?;
    let automation_class = js_sys::Reflect::get(&window, &"TestnetAutomation".into())
        .map_err(|_| "TestnetAutomation not found")?;
    
    let automation = js_sys::Reflect::construct(&automation_class.into(), &js_sys::Array::new())
        .map_err(|_| "Failed to create TestnetAutomation instance")?;
    
    let broadcast_fn = js_sys::Reflect::get(&automation, &"broadcastTransaction".into())
        .map_err(|_| "broadcastTransaction method not found")?;
    
    let promise = js_sys::Reflect::apply(
        &broadcast_fn.into(),
        &automation,
        &js_sys::Array::of1(&raw_tx.into()),
    )
    .map_err(|_| "Failed to call broadcastTransaction")?;
    
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("{:?}", e))?;
    
    let txid = js_sys::Reflect::get(&result, &"txid".into())
        .map_err(|_| "No txid in result")?
        .as_string()
        .ok_or("txid is not a string")?;
    
    let explorer_url = js_sys::Reflect::get(&result, &"explorerUrl".into())
        .map_err(|_| "No explorerUrl in result")?
        .as_string()
        .ok_or("explorerUrl is not a string")?;
    
    Ok((txid, explorer_url))
}
