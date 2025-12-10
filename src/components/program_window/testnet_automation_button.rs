use leptos::{component, create_rw_signal, create_signal, spawn_local, use_context, view, with, IntoView, SignalGet, SignalSet, SignalUpdate, SignalWith, SignalGetUntracked};
use web_sys::js_sys;
use hex_conservative::DisplayHex;

use crate::components::program_window::Program;
use crate::components::run_window::{SignedData, TxEnv};
use crate::util::{self, SigningKeys};

/// Inject witness signature into program text
/// Detects witness variable usage and injects the signature value
fn inject_witness_signature(program_text: &str, signature: &str) -> String {
    // Common witness variable names to look for
    let witness_vars = vec!["ALICE_SIGNATURE", "SIG", "SIGNATURE"];
    
    // Check if program uses any witness variables
    let mut used_witness_var = None;
    for var in &witness_vars {
        if program_text.contains(&format!("witness::{}", var)) {
            used_witness_var = Some(*var);
            break;
        }
    }
    
    let witness_var = match used_witness_var {
        Some(var) => var,
        None => "ALICE_SIGNATURE", // Default if none found
    };
    
    // Check if witness module exists
    if program_text.contains("mod witness") {
        // Find and update existing witness module
        let re = regex::Regex::new(r"mod\s+witness\s*\{[^}]*\}").unwrap();
        if let Some(mat) = re.find(program_text) {
            let witness_section = mat.as_str();
            
            // Check if signature already exists
            if witness_section.contains(witness_var) {
                // Replace existing signature value
                let sig_re = regex::Regex::new(&format!(
                    r"const\s+{}\s*:\s*Signature\s*=\s*0x[0-9a-fA-F]+;",
                    witness_var
                )).unwrap();
                
                if sig_re.is_match(witness_section) {
                    // Replace existing signature
                    let new_witness = sig_re.replace(
                        witness_section,
                        &format!("const {}: Signature = {};", witness_var, signature)
                    );
                    program_text.replace(witness_section, &new_witness)
                } else {
                    // Add signature to existing witness module
                    let new_witness = witness_section.replace(
                        "}",
                        &format!("    const {}: Signature = {};\n}}", witness_var, signature)
                    );
                    program_text.replace(witness_section, &new_witness)
                }
            } else {
                // Add signature to witness module
                let new_witness = if witness_section.trim() == "mod witness {}" {
                    format!("mod witness {{\n    const {}: Signature = {};\n}}", witness_var, signature)
                } else {
                    witness_section.replace(
                        "}",
                        &format!("    const {}: Signature = {};\n}}", witness_var, signature)
                    )
                };
                program_text.replace(witness_section, &new_witness)
            }
        } else {
            program_text.to_string()
        }
    } else {
        // Create new witness module at the beginning
        format!(
            "mod witness {{\n    const {}: Signature = {};\n}}\n\n{}",
            witness_var, signature, program_text
        )
    }
}

#[component]
pub fn TestnetAutomationButtons() -> impl IntoView {
    let program = use_context::<Program>().expect("program should exist in context");
    let tx_env = use_context::<TxEnv>().expect("transaction environment should exist in context");
    
    let (fund_status, set_fund_status) = create_signal(String::new());
    let (fund_loading, set_fund_loading) = create_signal(false);
    let (lookup_status, set_lookup_status) = create_signal(String::new());
    let (lookup_loading, set_lookup_loading) = create_signal(false);
    let (sign_status, set_sign_status) = create_signal(String::new());
    let (generated_signature, set_generated_signature) = create_signal(String::new());
    let (broadcast_status, set_broadcast_status) = create_signal(String::new());
    let (broadcast_loading, set_broadcast_loading) = create_signal(false);
    
    let funding_txid = create_rw_signal(String::new());
    let (current_address, set_current_address) = create_signal(String::new());
    
    // Get signing keys and signed data for sighash signature generation
    let signing_keys = use_context::<SigningKeys>().expect("signing keys should exist");
    let signed_data = use_context::<SignedData>().expect("signed data should exist");
    
    // Track generated signatures (sighash-based)
    let (signatures, set_signatures) = create_signal(Vec::<String>::new());
    
    // Step 1: Fund from faucet (using CORS proxy)
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

        set_current_address.set(address.clone());
        set_fund_loading.set(true);
        set_fund_status.set("Requesting funds via CORS proxy...".to_string());

        spawn_local(async move {
            match call_fund_from_faucet(&address).await {
                Ok(txid) => {
                    funding_txid.set(txid.clone());
                    set_fund_status.set(format!("✓ Funded! Txid: {}...", &txid[..16]));
                    set_fund_loading.set(false);
                }
                Err(err) => {
                    set_fund_status.set(format!("✗ {}",err));
                    set_fund_loading.set(false);
                }
            }
        });
    };
    
    // Step 3: Generate sighash signatures (implements Step 7 from official guide)
    // This generates signatures based on the transaction sighash and auto-injects them
    let generate_signatures = move |_| {
        set_sign_status.set("Generating sighash signatures...".to_string());
        
        // Get the transaction sighash (this is the message to sign)
        let message = signed_data.message.get();
        
        // Generate signature for Alice (key 0)
        let sig_alice = signing_keys.secret_keys[0].sign_schnorr(message);
        let sig_hex = format!("0x{}", sig_alice.serialize().as_hex());
        
        set_generated_signature.set(sig_hex.clone());
        
        // Auto-inject the signature into the program's witness section
        let current_text = program.text.get();
        let updated_text = inject_witness_signature(&current_text, &sig_hex);
        
        if updated_text != current_text {
            program.text.set(updated_text);
            // Trigger Monaco editor update if available
            if let Some(window) = web_sys::window() {
                if let Ok(update_fn) = js_sys::Reflect::get(&window, &"updateMonacoEditor".into()) {
                    if !update_fn.is_undefined() {
                        let _ = js_sys::Reflect::apply(
                            &update_fn.into(),
                            &window,
                            &js_sys::Array::of1(&program.text.get().into()),
                        );
                    }
                }
            }
            set_sign_status.set(format!("✓ Signature generated and injected: {}...", &sig_hex[..18]));
        } else {
            set_sign_status.set(format!("✓ Signature generated: {}...", &sig_hex[..18]));
        }
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
                Err(e) => {
                    set_broadcast_status.set(format!("✗ Transaction validation failed: {}. Check that Step 3 (signature generation) was completed and the witness data matches your program.", e));
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
                    let addr = current_address.get();
                    let txid = funding_txid.get();
                    if !addr.is_empty() || !txid.is_empty() {
                        view! {
                            <div class="step-data">
                                {if !addr.is_empty() {
                                    view! {
                                        <>
                                            <label>"Address:"</label>
                                            <input type="text" readonly value=addr />
                                        </>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none"></span> }.into_view()
                                }}
                                {if !txid.is_empty() {
                                    view! {
                                        <>
                                            <label>"Funding Txid:"</label>
                                            <input type="text" readonly value=txid />
                                        </>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none"></span> }.into_view()
                                }}
                            </div>
                        }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
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
                <button
                    class="workflow-button"
                    on:click=generate_signatures
                    disabled=move || !lookup_status.get().contains("Found") && !lookup_status.get().contains("Auto-filled")
                >
                    <i class="fas fa-key"></i>
                    " Generate Sighash Signatures"
                </button>
                {move || {
                    let sig = generated_signature.get();
                    if !sig.is_empty() {
                        view! {
                            <div class="step-data">
                                <label>"Signature (auto-injected into program):"</label>
                                <input
                                    type="text"
                                    readonly
                                    value=sig.clone()
                                    on:click=move |e| {
                                        let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                        target.select();
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.navigator().clipboard().write_text(&sig);
                                        }
                                    }
                                />
                                <p class="hint">"✓ Automatically injected into witness section (click to copy if needed)"</p>
                            </div>
                        }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
                {move || {
                    let status = sign_status.get();
                    if !status.is_empty() {
                        view! { <p class="step-status">{status}</p> }.into_view()
                    } else {
                        view! { <span style="display:none"></span> }.into_view()
                    }
                }}
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
