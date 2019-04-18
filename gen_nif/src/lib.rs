use llvm_sys as llvm;
use std::ptr;
use std::collections::HashMap;

use eir::{ Module as EirModule };
use eir::Atom;
use eir::op::OpKind;
use eir::{ FunctionIdent, Function };
use eir::Function as EirFunction;
use eir::AttributeKey;
use eir::Dialect;

use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::values::{ FunctionValue, ArrayValue, StructValue, PointerValue,
                       GlobalValue, AnyValueEnum, BasicValueEnum };
use inkwell::types::{ StructType, BasicType, BasicTypeEnum };
use inkwell::module::{ Module };

mod primitives;
mod emit;
mod nif_types;
mod target;
use target::nif::{ NifTarget, NifTypes };

use ::std::path::Path;
use ::std::io::Read;

#[derive(Debug, Clone)]
pub enum CallType {
    /// Takes an environment, arg*arity
    Function { arity: usize },
    /// Takes an environment, return term
    Continuation,
}

#[derive(Debug, Clone)]
pub struct CallSig {
    ident: FunctionIdent,
    typ: CallType,
}

pub struct Data {
    context: Context,
    module: Module,
    types: NifTypes,
    protos: HashMap<FunctionIdent, FunctionValue>,
    loc_id: u64,
}

pub struct CompilationContext {
    data: Data,
    target: NifTarget,
}

impl CompilationContext {

    pub fn new(llvm_module_name: &str) -> Self {
        let context = Context::create();
        let module = context.create_module(llvm_module_name);
        let target = NifTarget::new(&context, &module);
        let nif_types = NifTypes::new(&context, &module);
        CompilationContext {
            data: Data {
                context: context,
                module: module,
                types: nif_types,
                protos: HashMap::new(),
                loc_id: 1,
            },
            target: target,
        }
    }

    pub fn inner_mut<'a>(&'a mut self) -> (&'a mut Data, &'a mut NifTarget) {
        (&mut self.data, &mut self.target)
    }

    pub fn gen_proto(&mut self, fun: &FunctionIdent) {
        if fun.lambda.is_none() && !self.data.protos.contains_key(fun) {
            let val = self.target.gen_prototype_pub(&mut self.data, fun);
            self.data.protos.insert(fun.clone(), val);
        }
    }

    pub fn get_fun_value(&mut self, fun: &EirFunction) -> FunctionValue {
        let ident = fun.ident();
        if let Some(val) = self.data.protos.get(ident) {
            *val
        } else {
            let fun = self.target.gen_prototype(&mut self.data, fun);
            self.data.protos.insert(ident.clone(), fun);
            fun
        }
    }

    pub fn build_function(
        &mut self,
        module: &EirModule,
        name: &str,
        arity: usize
    ) -> bool
    {

        let funs: Vec<_> = module.functions.keys()
            .filter(|i| i.name.as_str() == name && i.arity == arity)
            .collect();

        for ident in funs.iter() {
            println!("LF: {}", ident);
            let fun = &module.functions[ident];
            assert!(fun.dialect() == Dialect::CPS);
            self.get_fun_value(fun);
        }

        for ident in funs.iter() {
            let fun = &module.functions[ident];
            let fn_val = self.get_fun_value(fun);

            crate::emit::emit_fun(
                &self.data.context,
                &self.data.module,
                &self.data.types,
                &self.data.protos,
                &mut self.data.loc_id,
                &module,
                fun,
                fn_val,
            );
        }

        true
    }

    pub fn print_ir(&self) {
        self.data.module.print_to_stderr();
    }

    pub fn write_bitcode(&self, path: &Path) {
        println!("{:?}", self.data.module.verify());
        self.data.module.write_bitcode_to_path(&path);
    }

    //fn gen_prototype(&mut self, ident: &FunctionIdent, continuation: bool) -> FunctionValue {
    //    if let Some(fun) = self.data.protos.get(ident) {
    //        *fun
    //    } else {
    //        let fun = self.target.gen_prototype(&mut self.data, ident);
    //        self.data.protos.insert(ident.clone(), fun);
    //        fun
    //    }
    //}

    //fn compile_function(&mut self, eir_module: &EirModule, ident: &FunctionIdent) {
    //    let fun = &eir_module.functions[ident];
    //    let proto = self.gen_prototype(ident);
    //}

}




//fn gen_prototype(context: &Context, module: &Module, nif_refs: &NifTypes,
//                 ident: &FunctionIdent) -> FunctionValue {
//    let bool_type = context.bool_type();
//
//    let mut arity = ident.arity;
//    if ident.lambda.is_some() { arity += 1; }
//
//    let mut args = vec![
//        nif_refs.term_ptr_type.into(),
//        nif_refs.env_ptr_type.into()
//    ];
//    args.extend((0..arity).map(|_| nif_refs.term_type));
//
//    let fn_type = bool_type.fn_type(&args, false);
//
//    let ident_mangled = mangle_ident(ident);
//    module.add_function(&ident_mangled, fn_type, None)
//}
//
//fn gen_prototypes(context: &Context, module: &Module, nif_refs: &NifTypes,
//                  fun: &Function, protos: &mut HashMap<FunctionIdent, FunctionValue>) {
//    let all_calls = fun.lir.get_all_calls();
//
//    if !protos.contains_key(&fun.ident) {
//        let val = gen_prototype(context, module, nif_refs, &fun.ident);
//        protos.insert(fun.ident.clone(), val);
//
//        for call in all_calls.iter() {
//            if !protos.contains_key(call) {
//                let val = gen_prototype(context, module, nif_refs, call);
//                protos.insert(call.clone(), val);
//            }
//        }
//    }
//
//}
//
//pub fn gen_module(eir: &EirModule, funs: &[FunctionIdent]) {
//
//    //let eir = {
//    //    let mut text = String::new();
//    //    std::fs::File::open("testing.core").unwrap()
//    //        .read_to_string(&mut text).unwrap();
//    //    let res = core_erlang_compiler::parser::parse(&text).unwrap();
//    //    core_erlang_compiler::ir::from_parsed(&res.0)
//    //};
//
//    let context = Context::create();
//    let module = context.create_module("my_module");
//    let void_type = context.void_type();
//
//    let nif_types = nif_types::make_nif_types(&context, &module);
//    let i64_type = context.i64_type();
//    let i32_type = context.i32_type();
//
//    let mut funcs = Vec::new();
//
//    let mut protos = HashMap::new();
//    for fun_ident in funs {
//        let mangled = mangle_ident(fun_ident);
//        println!("{}: {}", fun_ident, mangled);
//
//        let fun = &eir.functions[fun_ident];
//        gen_prototypes(&context, &module, &nif_types, fun, &mut protos);
//    }
//    println!("{:?}", protos);
//
//    for key in protos.keys() {
//        println!("{:?}", key);
//    }
//
//    for (ident, proto) in protos.iter() {
//        println!("{:?}", ident);
//        let fun = &eir.functions[ident];
//
//        emit_eir_fun(&context, &module, &nif_types, &protos, eir,
//                     fun, protos[ident]);
//        let wrapper = gen_wrapper(&context, &module, &nif_types,
//                                  ident, *proto);
//        funcs.push((ident.clone(), wrapper));
//    }
//
//    //let fn_type = i64_type.fn_type(&[
//    //    nif_types.term_ptr_type.into(),
//    //    nif_types.env_ptr_type.into(),
//    //    i32_type.into(),
//    //    i64_type.ptr_type(AddressSpace::Generic).into(),
//    //], false);
//    //let fn_val = module.add_function("foo_nif", fn_type, None);
//
//    let entry_t = EnifEntryT {
//        major: 2,
//        minor: 14,
//        name: eir.name.as_str().into(),
//        funcs: funcs.iter().map(|(ident, fun)| {
//            EnifFuncT {
//                name: ident.name.as_str().to_string(),
//                arity: ident.arity as u32,
//                fun: fun.as_global_value().as_pointer_value(),
//                flags: 0,
//            }
//        }).collect(),
//        load: Some(
//            nif_types.on_load
//                .as_global_value().as_pointer_value()),
//        reload: None,
//        upgrade: None,
//        unload: None,
//        vm_variant: "beam.vanilla".into(),
//        options: 0,
//        sizeof_erlnifresourcetypeinit: 0,
//        min_erts: "0.0".into(),
//    };
//    let nif_entry = make_enif_entry_t(&context, &module, &entry_t);
//    let nif_entry_pointer = nif_entry.as_pointer_value();
//
//    // Make nif init
//    let nif_init_fn_type = (nif_entry_pointer.get_type()).fn_type(&[], false);
//    let nif_init_fn_val = module.add_function("nif_init", nif_init_fn_type, None);
//    let builder = context.create_builder();
//    {
//        let basic_block = context.append_basic_block(&nif_init_fn_val, "entry");
//        builder.position_at_end(&basic_block);
//
//        builder.build_return(Some(&nif_entry_pointer));
//    }
//
//    println!("{:?}", module.verify());
//
//    let path = Path::new("module.bc");
//    module.write_bitcode_to_path(&path);
//}
