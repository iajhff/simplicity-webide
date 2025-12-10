use leptos::{component, create_rw_signal, create_signal, spawn_local, use_context, view, with, IntoView, SignalGet, SignalSet, SignalUpdate};
use web_sys::js_sys;
use simplicityhl::elements::secp256k1_zkp as secp256k1;

use crate::components::program_window::Program;
use crate::components::run_window::{SignedData, TxEnv};
use crate::util::{self, SigningKeys};

/// Detect all witness variables used in the program
fn detect_witness_variables(program_text: &str) -> Vec<(String, String)> {
    let mut witness_vars = Vec::new();
    
    // Regex to find witness::VARIABLE_NAME and infer type from context
    let witness_re = regex::Regex::new(r"witness::([A-Z_][A-Z0-9_]*)").unwrap();
    
    for cap in witness_re.captures_iter(program_text) {
        let var_name = cap[1].to_string();
        
        // Infer type from variable name or context
        let var_type = if var_name.contains("SIGNATURE") || var_name == "SIG" {
            "Signature"
        } else if var_name.contains("PUBLIC_KEY") || var_name.contains("PUBKEY") {
            "Pubkey"
        } else {
            // Default to signature for safety
            "Signature"
        };
        
        // Avoid duplicates
        if !witness_vars.iter().any(|(name, _)| name == &var_name) {
            witness_vars.push((var_name, var_type.to_string()));
        }
    }
    
    witness_vars
}

/// Generate witness values based on detected variables
fn generate_witness_values(
    witness_vars: &[(String, String)],
    signing_keys: &SigningKeys,
    sighash: secp256k1::Message,
) -> Vec<(String, String, String)> {
    use hex_conservative::DisplayHex;
    let mut generated = Vec::new();
    
    for (var_name, var_type) in witness_vars {
        match var_type.as_str() {
            "Signature" => {
                // Generate signature using Alice's key (index 0)
                let sig = signing_keys.secret_keys[0].sign_schnorr(sighash);
                let sig_hex = format!("0x{}", sig.serialize().as_hex());
                generated.push((var_name.clone(), var_type.clone(), sig_hex));
            }
            "Pubkey" => {
                // Use Alice's public key
                let pk = signing_keys.secret_keys[0].x_only_public_key().0;
                let pk_hex = format!("0x{}", pk.serialize().as_hex());
                generated.push((var_name.clone(), var_type.clone(), pk_hex));
            }
            _ => {
                // Unsupported type - skip or add placeholder
                generated.push((
                    var_name.clone(),
                    var_type.clone(),
                    "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                ));
            }
        }
    }
    
    generated
}

/// Inject witness values into program text
fn inject_witness_values(program_text: &str, witness_values: &[(String, String, String)]) -> String {
    if witness_values.is_empty() {
        return program_text.to_string();
    }
    
    // Build witness module content
    let mut witness_content = String::from("mod witness {\n");
    for (var_name, var_type, value) in witness_values {
        witness_content.push_str(&format!("    const {}: {} = {};\n", var_name, var_type, value));
    }
    witness_content.push_str("}");
    
    // Check if witness module exists
    if program_text.contains("mod witness") {
        // Replace existing witness module
        let re = regex::Regex::new(r"mod\s+witness\s*\{[^}]*\}").unwrap();
        if let Some(mat) = re.find(program_text) {
            program_text.replace(mat.as_str(), &witness_content)
        } else {
            program_text.to_string()
        }
    } else {
        // Create new witness module at the beginning
        format!("{}\n\n{}", witness_content, program_text)
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
    
    // Step 3: Generate witness data (implements Step 7 from official guide)
    // Detects required witness variables and generates appropriate values
    let generate_signatures = move |_| {
        set_sign_status.set("Analyzing program witness requirements...".to_string());
        
        // Get the transaction sighash (this is the message to sign)
        let message = signed_data.message.get();
        
        // Detect what witness variables the program needs
        let current_text = program.text.get();
        let witness_vars = detect_witness_variables(&current_text);
        
        if witness_vars.is_empty() {
            set_sign_status.set("✗ No witness variables found in program. Add witness::ALICE_SIGNATURE or similar to your main function.".to_string());
            return;
        }
        
        // Generate values for detected witness variables
        let witness_values = generate_witness_values(&witness_vars, &signing_keys, message);
        
        // Display what was generated
        let summary: Vec<String> = witness_values.iter()
            .map(|(name, typ, val)| format!("{}:{} = {}...", name, typ, &val[..std::cmp::min(18, val.len())]))
            .collect();
        
        // Store first signature for display
        if let Some((_, _, sig)) = witness_values.iter().find(|(_, typ, _)| typ == "Signature") {
            set_generated_signature.set(sig.clone());
        }
        
        // Auto-inject all witness values into the program
        let updated_text = inject_witness_values(&current_text, &witness_values);
        
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
            set_sign_status.set(format!("✓ Generated {} witness value(s): {}", witness_values.len(), summary.join(", ")));
        } else {
            set_sign_status.set("✓ Witness values generated (no changes needed)".to_string());
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
                    <h4>"Generate Witness Data"</h4>
                </div>
                <button
                    class="workflow-button"
                    on:click=generate_signatures
                    disabled=move || !lookup_status.get().contains("Found") && !lookup_status.get().contains("Auto-filled")
                >
                    <i class="fas fa-key"></i>
                    " Generate Witness Values"
                </button>
                {move || {
                    let sig = generated_signature.get();
                    if !sig.is_empty() {
                        view! {
                            <div class="step-data">
                                <label>"Primary Signature:"</label>
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
                                <p class="hint">"✓ All witness values auto-injected into program (click to copy)"</p>
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
