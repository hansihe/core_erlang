use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashMap;

use num_bigint::BigInt;

use ::{ SSAVariable, LabelN, Atom, LambdaEnvIdx, FunctionIdent, Source,
        AtomicLiteral, OpKind, BoundLambdaEnv, BasicBlock, Module };
use ::term::{ Term, TermType, Pid };
use ::vm::VMState;
use ::module::ModuleType;
use ::pattern::CaseContext;

mod exec;

#[derive(Debug, Clone)]
pub enum CallReturn {
    Return { term: Term },
    Throw,
}
impl CallReturn {

    pub fn unwrap_return<'a>(&'a self) -> &'a Term {
        match self {
            CallReturn::Return { ref term } => term,
            _ => panic!("Expected return"),
        }
    }

}

#[derive(Copy, Clone, PartialEq, Eq)]
enum StackFrameState {
    Normal,
    InCall(CallOutcomes),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CallOutcomes {
    ret_ok_ssa: SSAVariable,
    ret_ok_slot: usize,
    ret_ok_label: Option<LabelN>,
    ret_throw_ssa: SSAVariable,
    ret_throw_slot: usize,
    ret_throw_label: Option<LabelN>,
}


#[derive(Clone, Debug)]
pub enum BlockResult {
    Branch { slot: usize },
    Return { ret: CallReturn },
    Call {
        module: Atom,
        fun: Atom,
        args: Vec<Term>,
        lambda: Option<(LambdaEnvIdx, usize)>,
        outcomes: CallOutcomes,
    },
    Suspend,
}

pub struct ProcessContext {
    pub stack: Rc<RefCell<Vec<StackFrameType>>>,
    pub return_val: Option<CallReturn>,
    pub pid: Pid,
}

impl ProcessContext {

    pub fn new(pid: Pid) -> Self {
        ProcessContext {
            stack: Rc::new(RefCell::new(Vec::new())),
            return_val: None,
            pid: pid,
        }
    }

    pub fn make_call_stackframe(&self, vm: &VMState,
                            module: Atom, fun_ident: FunctionIdent,
                            args: Vec<Term>) -> StackFrameType {

        assert!(vm.modules.contains_key(module.as_str()));
        let module_t = vm.modules.get(module.as_str()).unwrap();

        assert!(fun_ident.arity == args.len() as u32);

        println!("-> {}:{}", module, fun_ident);

        match module_t {
            ModuleType::Erlang(c_module, native_overlay_opt) => {
                if let Some(native_overlay) = native_overlay_opt {
                    if native_overlay.functions.contains_key(&(
                        fun_ident.name.as_str().to_string(),
                        fun_ident.arity as u32
                    )) {
                        let native_frame = NativeStackFrame {
                            module,
                            fun_ident,
                            args,
                        };
                        return StackFrameType::Native(native_frame);
                    }
                }

                let fun = c_module.functions.iter()
                    .find(|f| f.ident == fun_ident)
                    .unwrap();
                let c_lir = fun.lir_function.as_ref().unwrap();

                let mut call_frame = StackFrame::new(
                    module.clone(),
                    fun_ident.clone(),
                    c_lir.entry()
                );

                // Insert arguments into frame
                assert!(c_lir.args.len() == args.len());
                for (var_def, term) in c_lir.args.iter().zip(args) {
                    call_frame.write(*var_def, term.clone());
                }

                StackFrameType::Erlang(call_frame)
            }
            ModuleType::Native(native) => {
                if native.functions.contains_key(&(
                    fun_ident.name.as_str().to_string(),
                    fun_ident.arity as u32
                )) {
                    let native_frame = NativeStackFrame {
                        module,
                        fun_ident,
                        args,
                    };
                    StackFrameType::Native(native_frame)
                } else {
                    panic!()
                }
            }
        }
    }

    // In our case we count a single basic block as a reduction.
    // This should be a decent way to do things for a reference
    // implementation.
    pub fn do_reduction(&mut self, vm: &VMState) -> bool {
        let mut push_frame_parts: Option<(Atom, FunctionIdent, Vec<Term>)> = None;
        let mut pop_frame = false;
        let mut suspend = false;

        if self.stack.borrow().len() == 0 {
            return true;
        }

        {
            let stack_i = self.stack.clone();
            let mut stack = stack_i.borrow_mut();
            let frame = stack.last_mut().unwrap();
            match frame {
                StackFrameType::Erlang(frame) => {

                    // If we were in a call, handle result
                    if self.return_val.is_some() {
                        let ret = self.return_val.take().unwrap();
                        if let StackFrameState::InCall(outcomes) = frame.state {
                            match ret {
                                CallReturn::Return { term } => {
                                    frame.prev_basic_block = Some(frame.basic_block);
                                    frame.basic_block = outcomes.ret_ok_label.unwrap();
                                    frame.write(outcomes.ret_ok_ssa, term);
                                    println!("Branching to slot {}", outcomes.ret_ok_slot);
                                }
                                CallReturn::Throw => {
                                    frame.prev_basic_block = Some(frame.basic_block);
                                    frame.basic_block = outcomes.ret_throw_label.unwrap();
                                    // TODO TODO
                                    let replacement_term = Term::ValueList(
                                        vec![Term::Nil, Term::Nil]
                                    );
                                    frame.write(outcomes.ret_throw_ssa, replacement_term);
                                    println!("Branching to slot {}", outcomes.ret_throw_slot);
                                }
                            }
                            frame.state = StackFrameState::Normal;
                        } else {
                            panic!();
                        }
                    } else {
                        assert!(frame.state == StackFrameState::Normal);
                    }

                    let curr_block_id = frame.basic_block;

                    assert!(vm.modules.contains_key(frame.module.as_str()));
                    let module_t = vm.modules.get(frame.module.as_str()).unwrap();

                    if let ModuleType::Erlang(ref module, _native_overlay_opt) = module_t {

                        let fun = module.functions.iter()
                            .find(|fun| fun.ident == frame.function)
                            .unwrap();
                        let lir = fun.lir_function.as_ref().unwrap();
                        let block = lir.block(frame.basic_block);

                        match frame.exec_block(module, block) {
                            BlockResult::Branch { slot } => {
                                let slots = lir.branch_slots(curr_block_id);
                                frame.prev_basic_block = Some(curr_block_id);
                                frame.basic_block = slots[slot];
                                println!("Branching to slot {}", slot);
                            }
                            BlockResult::Call { module, fun, args, lambda, mut outcomes } => {
                                let slots = lir.branch_slots(curr_block_id);
                                outcomes.ret_ok_label = Some(slots[outcomes.ret_ok_slot]);
                                outcomes.ret_throw_label = Some(slots[outcomes.ret_throw_slot]);

                                let ident = FunctionIdent {
                                    name: fun,
                                    arity: args.len() as u32,
                                    lambda: lambda,
                                };
                                push_frame_parts = Some((module, ident, args));
                                frame.state = StackFrameState::InCall(outcomes);
                            }
                            BlockResult::Return { ret } => {
                                self.return_val = Some(ret);
                                pop_frame = true;
                                println!("<- {}:{}", module.name, frame.function);
                            }
                            BlockResult::Suspend => {
                                suspend = true;
                                println!("======== SUSPENDED BY RESULT ========");
                            }
                        }

                    } else {
                        unreachable!()
                    }

                }
                StackFrameType::Native(frame) => {
                    assert!(vm.modules.contains_key(frame.module.as_str()));
                    let module_t = vm.modules.get(frame.module.as_str()).unwrap();

                    match module_t {
                        ModuleType::Native(module) => {
                            let fun = &module.functions[&(
                                frame.fun_ident.name.as_str().to_string(),
                                frame.fun_ident.arity as u32
                            )];
                            self.return_val = Some((fun)(vm, self, &frame.args));
                            pop_frame = true;
                            println!("<- {}:{}", module.name, frame.fun_ident);
                        }
                        ModuleType::Erlang(_mod, Some(module)) => {
                            let fun = &module.functions[&(
                                frame.fun_ident.name.as_str().to_string(),
                                frame.fun_ident.arity as u32
                            )];
                            self.return_val = Some((fun)(vm, self, &frame.args));
                            pop_frame = true;
                            println!("<- {}:{}", module.name, frame.fun_ident);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }

        if push_frame_parts.is_some() {
            let (module, ident, args) = push_frame_parts.take().unwrap();
            let frame = self.make_call_stackframe(vm, module, ident, args);
            let mut stack = self.stack.borrow_mut();
            stack.push(frame);
        }
        if pop_frame {
            let mut stack = self.stack.borrow_mut();
            stack.pop();
        }

        suspend
    }

    pub fn run_reductions(&mut self, vm: &mut VMState, reductions: u64) {
        let reduction_counter = 0;
        let stack_len = self.stack.borrow().len();
        while reduction_counter < reductions && stack_len > 0 {
            if self.do_reduction(vm) {
                break;
            }
        }
    }

}

pub struct NativeStackFrame {
    module: Atom,
    fun_ident: FunctionIdent,
    args: Vec<Term>,
}

pub enum StackFrameType {
    Erlang(StackFrame),
    Native(NativeStackFrame),
}

pub struct StackFrame {
    variables: HashMap<SSAVariable, Term>,
    state: StackFrameState,
    module: Atom,
    function: FunctionIdent,
    basic_block: LabelN,
    prev_basic_block: Option<LabelN>,
}
impl StackFrame {

    fn new(module: Atom, function: FunctionIdent, label: LabelN) -> Self {
        StackFrame {
            variables: HashMap::new(),
            state: StackFrameState::Normal,
            function: function,
            basic_block: label,
            module: module,
            prev_basic_block: None,
        }
    }

    fn write(&mut self, ssa: SSAVariable, term: Term) {
        //self.tombstones.remove(&ssa);
        self.variables.insert(ssa, term);
    }

    fn read(&self, src: &Source) -> Term {
        match *src {
            Source::Variable(ref var) => {
                if let Some(term) = self.variables.get(var) {
                    term.clone()
                } else {
                    panic!("var {:?} not found in frame", var);
                }
            }
            Source::Constant(ref literal) => {
                match *literal {
                    AtomicLiteral::Atom(ref atom) => Term::Atom(atom.clone()),
                    AtomicLiteral::Integer(ref inner) => {
                        let mut bi = BigInt::parse_bytes(inner.digits.as_bytes(), 10)
                            .unwrap();
                        if !inner.sign {
                            bi *= -1;
                        }
                        Term::Integer(bi)
                    },
                    AtomicLiteral::Nil => Term::Nil,
                    _ => unimplemented!(),
                }
            }
        }
    }

    fn tombstone(&mut self, ssa: SSAVariable) {
        //self.tombstones.insert(ssa);
        self.variables.remove(&ssa);
    }


}
