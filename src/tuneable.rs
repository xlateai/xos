use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock, RwLock};
use std::fmt::Debug;

use syn::{visit_mut::VisitMut, Expr, ExprAssign, ExprPath, File, Item, ItemMacro};
use quote::ToTokens;
use proc_macro2::TokenStream;

/// Trait all tuneables implement
pub trait TuneableEntry: Send + Sync {
    fn file(&self) -> &'static str;
    fn line(&self) -> u32;
    fn column(&self) -> u32;
    fn name(&self) -> &'static str;
    fn write_source_line(&self) -> String;
    fn get_dynamic_value(&self) -> String;
}

static REGISTRY: OnceLock<Mutex<Vec<&'static dyn TuneableEntry>>> = OnceLock::new();

pub fn register(entry: &'static dyn TuneableEntry) {
    REGISTRY.get_or_init(Default::default).lock().unwrap().push(entry);
}

pub fn write_all_to_source() {
    let Some(registry) = REGISTRY.get() else { return };
    let registry = registry.lock().unwrap();

    let mut by_file: HashMap<&'static str, HashMap<&'static str, &dyn TuneableEntry>> = HashMap::new();
    for entry in registry.iter() {
        by_file
            .entry(entry.file())
            .or_default()
            .insert(entry.name(), *entry);
    }

    for (file, entries) in by_file {
        let path = Path::new(file);
        let Ok(code) = fs::read_to_string(path) else { continue };
        let Ok(mut syntax) = syn::parse_file(&code) else { continue };

        let mut visitor = TuneableUpdater { entry_map: &entries };
        visitor.visit_file_mut(&mut syntax);

        let updated_code = prettyplease::unparse(&syntax);
        let _ = fs::write(path, updated_code);
    }
}

struct TuneableUpdater<'a> {
    entry_map: &'a HashMap<&'static str, &'static dyn TuneableEntry>,
}

impl<'a> VisitMut for TuneableUpdater<'a> {
    fn visit_item_macro_mut(&mut self, mac: &mut ItemMacro) {
        if let Some(ident) = mac.mac.path.get_ident() {
            if ident == "tuneables" {
                // Attempt to parse macro body as token tree (not Block!)
                let tokens = mac.mac.tokens.clone();
                let stmts = match syn::parse2::<syn::Block>(tokens.clone()).map(|b| b.stmts) {
                    Ok(stmts) => stmts,
                    Err(_) => return,
                };

                let mut new_tokens = proc_macro2::TokenStream::new();

                for stmt in stmts {
                    if let syn::Stmt::Expr(expr, Some(semi)) = stmt {
                        let orig_expr = expr.clone();
                        match expr {
                            Expr::Assign(mut assign) => {
                                if let Expr::Path(ExprPath { path, .. }) = *assign.left.clone() {
                                    if let Some(ident) = path.get_ident() {
                                        if let Some(entry) = self.entry_map.get(ident.to_string().as_str()) {
                                            let new_rhs: Expr = syn::parse_str(&entry.get_dynamic_value()).unwrap();
                                            assign.right = Box::new(new_rhs);
                                            assign.to_tokens(&mut new_tokens);
                                            semi.to_tokens(&mut new_tokens);
                                            continue;
                                        }
                                    }
                                }

                                // fallback to original assignment
                                orig_expr.to_tokens(&mut new_tokens);
                                semi.to_tokens(&mut new_tokens);
                            }
                            other => {
                                other.to_tokens(&mut new_tokens);
                                semi.to_tokens(&mut new_tokens);
                            }
                        }
                    } else {
                        stmt.to_tokens(&mut new_tokens);
                    }
                }

                mac.mac.tokens = quote::quote! { { #new_tokens } };
            }
        }

        syn::visit_mut::visit_item_macro_mut(self, mac);
    }
}


pub struct Tuneable<T: Copy + Debug + 'static> {
    name: &'static str,
    file: &'static str,
    line: u32,
    column: u32,
    value: RwLock<T>,
}

impl<T: Copy + Debug + 'static> Tuneable<T> {
    pub const fn new(name: &'static str, value: T, file: &'static str, line: u32, column: u32) -> Self {
        Self {
            name,
            file,
            line,
            column,
            value: RwLock::new(value),
        }
    }

    pub fn get(&self) -> T {
        *self.value.read().unwrap()
    }

    pub fn set(&self, v: T) {
        *self.value.write().unwrap() = v;
    }
}

macro_rules! impl_tuneable_entry {
    ($t:ty) => {
        impl TuneableEntry for Tuneable<$t> {
            fn file(&self) -> &'static str { self.file }
            fn line(&self) -> u32 { self.line }
            fn column(&self) -> u32 { self.column }
            fn name(&self) -> &'static str { self.name }
            fn write_source_line(&self) -> String {
                format!("    {}: {} = {:?};", self.name, stringify!($t), self.get())
            }
            fn get_dynamic_value(&self) -> String {
                format!("{:?}", self.get())
            }
        }
    };
}

impl_tuneable_entry!(f32);
impl_tuneable_entry!(i32);
impl_tuneable_entry!(bool);
impl_tuneable_entry!(f64);
impl_tuneable_entry!(u32);

#[macro_export]
macro_rules! tuneables {
    ($($ident:ident : $ty:ty = $val:expr;)*) => {
        $(
            #[allow(non_snake_case)]
            pub fn $ident() -> &'static $crate::tuneable::Tuneable<$ty> {
                use std::sync::LazyLock;
                static INSTANCE: LazyLock<&'static $crate::tuneable::Tuneable<$ty>> = LazyLock::new(|| {
                    static INNER: $crate::tuneable::Tuneable<$ty> = $crate::tuneable::Tuneable::new(
                        stringify!($ident),
                        $val,
                        file!(),
                        line!(),
                        column!(),
                    );
                    $crate::tuneable::register(&INNER);
                    &INNER
                });
                *INSTANCE
            }
        )*
    };
}
