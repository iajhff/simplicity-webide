use leptos::{component, create_rw_signal, create_signal, spawn_local, use_context, view, with, IntoView, SignalGet, SignalSet, SignalUpdate, SignalWith};
use web_sys::js_sys;
use simplicityhl::elements::secp256k1_zkp as secp256k1;
use simplicityhl::{WitnessValues, Value};
use simplicityhl::str::WitnessName;
use std::collections::HashMap;
use hex_conservative::DisplayHex;

use crate::components::program_window::Program;
use crate::components::run_window::{SignedData, TxEnv};
use crate::util::{self, SigningKeys};

/// Analyze program structure to infer contract type and requirements
fn analyze_program_structure(program_text: &str) -> ProgramAnalysis {
    let mut analysis = ProgramAnalysis {
        has_signature_verification: false,
        has_hash_check: false,
        has_timelock: false,
        has_conditional: false,
        signature_count: 0,
        uses_preimage: false,
    };
    
    // Detect signature verification
    if program_text.contains("jet::bip_0340_verify") {
        analysis.has_signature_verification = true;
        // Count how many times verify is called
        analysis.signature_count = program_text.matches("jet::bip_0340_verify").count();
    }
    
    // Detect hash operations (might need preimage)
    if program_text.contains("jet::sha_256") || program_text.contains("jet::eq_256") {
        analysis.has_hash_check = true;
    }
    
    // Detect timelock
    if program_text.contains("jet::check_lock_time") || program_text.contains("jet::check_sequence") {
        analysis.has_timelock = true;
    }
    
    // Detect conditional logic (might need sum types)
    if program_text.contains("match ") || program_text.contains("if ") {
        analysis.has_conditional = true;
    }
    
    // Detect preimage usage
    if program_text.contains("preimage") {
        analysis.uses_preimage = true;
    }
    
    analysis
}

#[derive(Debug)]
struct ProgramAnalysis {
    has_signature_verification: bool,
    has_hash_check: bool,
    has_timelock: bool,
    has_conditional: bool,
    signature_count: usize,
    uses_preimage: bool,
}

/// Detect all witness variables used in the program with smart type inference
fn detect_witness_variables(program_text: &str) -> Vec<(String, String)> {
    let mut witness_vars = Vec::new();
    
    // Analyze program to understand what it does
    let analysis = analyze_program_structure(program_text);
    
    // Regex to find witness::VARIABLE_NAME
    let witness_re = regex::Regex::new(r"witness::([A-Z_][A-Z0-9_]*)").unwrap();
    
    for cap in witness_re.captures_iter(program_text) {
        let var_name = cap[1].to_string();
        
        // Smart type inference based on:
        // 1. Variable name
        // 2. Program structure analysis
        // 3. Context of usage
        let var_type = infer_witness_type(&var_name, &analysis, program_text);
        
        // Avoid duplicates
        if !witness_vars.iter().any(|(name, _)| name == &var_name) {
            witness_vars.push((var_name, var_type));
        }
    }
    
    witness_vars
}

/// Infer witness variable type from name and program context
fn infer_witness_type(var_name: &str, analysis: &ProgramAnalysis, program_text: &str) -> String {
    // Check for multi-signature patterns
    if var_name.contains("2_OF_3") || var_name.contains("MULTISIG") || var_name.contains("_OF_") {
        return "MultiSig".to_string(); // Complex array of optional signatures
    }
    
    // Check variable name patterns
    if var_name.contains("SIGNATURE") || var_name == "SIG" || var_name.ends_with("_SIG") {
        // If multiple signatures are needed, it might be an array
        if analysis.signature_count > 1 {
            return "MultiSig".to_string();
        }
        return "Signature".to_string();
    }
    
    if var_name.contains("PUBLIC_KEY") || var_name.contains("PUBKEY") || var_name == "PK" {
        return "Pubkey".to_string();
    }
    
    if var_name.contains("PREIMAGE") || var_name.contains("SECRET") {
        return "u256".to_string(); // Preimages are typically u256
    }
    
    // Check if it's used in a conditional (might be sum type)
    if analysis.has_conditional {
        // Look for the variable in match or if expressions
        let var_ref = format!("witness::{}", var_name);
        if program_text.contains(&format!("match {}", var_ref)) 
            || program_text.contains(&format!("unwrap_left({})", var_ref)) 
            || program_text.contains(&format!("unwrap_right({})", var_ref)) {
            return "SumType".to_string(); // Complex sum type - needs manual definition
        }
    }
    
    // Default: if program has signature verification, assume it's a signature
    if analysis.has_signature_verification {
        "Signature".to_string()
    } else {
        "u256".to_string() // Generic fallback
    }
}

/// Generate witness/param values based on detected variables
fn generate_witness_values(
    witness_vars: &[(String, String)],
    signing_keys: &SigningKeys,
    sighash: secp256k1::Message,
    selected_key_indices: &[usize],
) -> Result<(Vec<(String, String, String)>, bool), String> {
    use hex_conservative::DisplayHex;
    let mut generated = Vec::new();
    let mut unsupported = Vec::new();
    let mut needs_multisig_selection = false;
    
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
            "MultiSig" => {
                // For multisig, generate individual signature params if keys are selected
                if selected_key_indices.is_empty() {
                    needs_multisig_selection = true;
                } else {
                    // Generate individual signatures for selected keys
                    for &key_idx in selected_key_indices {
                        let key_name = get_key_name(key_idx);
                        let sig = signing_keys.secret_keys[key_idx].sign_schnorr(sighash);
                        let sig_hex = format!("0x{}", sig.serialize().as_hex());
                        generated.push((
                            format!("{}_SIGNATURE", key_name.to_uppercase()),
                            "Signature".to_string(),
                            sig_hex
                        ));
                    }
                }
            }
            "u256" => {
                // For preimages or generic u256, use placeholder
                unsupported.push(format!("{}:u256 (preimage/hash - requires manual input)", var_name));
            }
            "SumType" => {
                // Complex sum type - cannot auto-generate
                unsupported.push(format!("{}:SumType (complex type - requires manual definition)", var_name));
            }
            _ => {
                unsupported.push(format!("{}:{} (unknown type)", var_name, var_type));
            }
        }
    }
    
    if !unsupported.is_empty() {
        return Err(format!(
            "Cannot auto-generate: {}. Please define manually.",
            unsupported.join(", ")
        ));
    }
    
    Ok((generated, needs_multisig_selection))
}

fn get_key_name(index: usize) -> &'static str {
    match index {
        0 => "ALICE",
        1 => "BOB",
        2 => "CHARLIE",
        3 => "DAVID",
        4 => "EVE",
        5 => "FRANK",
        6 => "GRACE",
        7 => "HEIDI",
        8 => "IVAN",
        9 => "JUDY",
        _ => "KEY",
    }
}

/// Inject values into program text (as params for multisig, as witness for simple)
fn inject_witness_values(program_text: &str, witness_values: &[(String, String, String)], as_params: bool) -> String {
    if witness_values.is_empty() {
        return program_text.to_string();
    }
    
    if as_params {
        // Inject as params (for multisig - user constructs witness from these)
        let mut param_content = String::from("mod param {\n");
        // Keep existing params
        if let Some(existing_params) = extract_module_content(program_text, "param") {
            param_content.push_str(&existing_params);
        }
        // Add new signature params
        for (var_name, var_type, value) in witness_values {
            param_content.push_str(&format!("    const {}: {} = {};\n", var_name, var_type, value));
        }
        param_content.push_str("}");
        
        // Replace or insert param module
        if program_text.contains("mod param") {
            let re = regex::Regex::new(r"mod\s+param\s*\{[^}]*\}").unwrap();
            if let Some(mat) = re.find(program_text) {
                program_text.replace(mat.as_str(), &param_content)
            } else {
                program_text.to_string()
            }
        } else {
            format!("{}\n\n{}", param_content, program_text)
        }
    } else {
        // Inject as witness (for simple cases)
        let mut witness_content = String::from("mod witness {\n");
        for (var_name, var_type, value) in witness_values {
            witness_content.push_str(&format!("    const {}: {} = {};\n", var_name, var_type, value));
        }
        witness_content.push_str("}");
        
        if program_text.contains("mod witness") {
            let re = regex::Regex::new(r"mod\s+witness\s*\{[^}]*\}").unwrap();
            if let Some(mat) = re.find(program_text) {
                program_text.replace(mat.as_str(), &witness_content)
            } else {
                program_text.to_string()
            }
        } else {
            format!("{}\n\n{}", witness_content, program_text)
        }
    }
}

fn extract_module_content(program_text: &str, module_name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r"mod\s+{}\s*\{{([^}}]*)\}}", module_name)).ok()?;
    re.captures(program_text).and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
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
    let (spending_txid, set_spending_txid) = create_signal(String::new());
    
    // Witness value management
    let (detected_witness_vars, set_detected_witness_vars) = create_signal(Vec::<(String, String)>::new());
    let (witness_field_values, set_witness_field_values) = create_signal(HashMap::<String, String>::new());
    
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
        let witness_values = match generate_witness_values(&witness_vars, &signing_keys, message) {
            Ok(values) => values,
            Err(err) => {
                set_sign_status.set(format!("⚠ {}", err));
                return;
            }
        };
        
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
                Ok((spending_tx, explorer_url)) => {
                    // Log for debugging
                    web_sys::console::log_1(&format!("✓ Spending transaction broadcast! Txid: {}", spending_tx).into());
                    web_sys::console::log_1(&format!("Explorer URL: {}", explorer_url).into());
                    
                    set_spending_txid.set(spending_tx.clone());
                    set_broadcast_status.set(format!("✓ Spending transaction broadcast! Txid: {}", spending_tx));
                    set_broadcast_loading.set(false);
                    
                    // Open the SPENDING transaction (not the funding transaction)
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
                                            <input type="text" readonly value=addr.clone() on:click=move |e| {
                                                let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                                target.select();
                                            } />
                                        </>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none"></span> }.into_view()
                                }}
                                {if !txid.is_empty() {
                                    let funding_url = format!("https://blockstream.info/liquidtestnet/tx/{}", txid);
                                    view! {
                                        <>
                                            <label>"Funding Txid:"</label>
                                            <input type="text" readonly value=txid.clone() on:click=move |e| {
                                                let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                                target.select();
                                            } />
                                            <button
                                                class="workflow-button"
                                                style="margin-top: 8px;"
                                                on:click=move |_| {
                                                    if let Some(window) = web_sys::window() {
                                                        let _ = window.open_with_url_and_target(&funding_url, "_blank");
                                                    }
                                                }
                                            >
                                                <i class="fas fa-external-link-alt"></i>
                                                " View Funding Transaction"
                                            </button>
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
                {move || {
                    let txid = spending_txid.get();
                    if !txid.is_empty() {
                        let explorer_url = format!("https://blockstream.info/liquidtestnet/tx/{}", txid);
                        view! {
                            <div class="step-data">
                                <label>"Spending Transaction:"</label>
                                <input
                                    type="text"
                                    readonly
                                    value=txid.clone()
                                    on:click=move |e| {
                                        let target = leptos::event_target::<web_sys::HtmlInputElement>(&e);
                                        target.select();
                                    }
                                />
                                <button
                                    class="workflow-button"
                                    style="margin-top: 10px;"
                                    on:click=move |_| {
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.open_with_url_and_target(&explorer_url, "_blank");
                                        }
                                    }
                                >
                                    <i class="fas fa-external-link-alt"></i>
                                    " View Spending Transaction"
                                </button>
                            </div>
                        }.into_view()
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
