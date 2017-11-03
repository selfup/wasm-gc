extern crate parity_wasm;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::env;
use std::collections::{BTreeSet, HashSet};

use parity_wasm::elements::*;

fn main() {
    env_logger::init().unwrap();

    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        println!("Usage: {} input.wasm output.wasm", args[0]);
        return
    }

    let mut module = parity_wasm::deserialize_file(&args[1])
        .expect("Failed to load module");

    let analysis = {
        let mut cx = LiveContext::new(&module);

        cx.blacklist.insert("main");
        cx.blacklist.insert("__ashldi3");
        cx.blacklist.insert("__ashlti3");
        cx.blacklist.insert("__ashrdi3");
        cx.blacklist.insert("__ashrti3");
        cx.blacklist.insert("__lshrdi3");
        cx.blacklist.insert("__lshrti3");
        cx.blacklist.insert("__floatsisf");
        cx.blacklist.insert("__floatsidf");
        cx.blacklist.insert("__floatdidf");
        cx.blacklist.insert("__floattisf");
        cx.blacklist.insert("__floattidf");
        cx.blacklist.insert("__floatunsisf");
        cx.blacklist.insert("__floatunsidf");
        cx.blacklist.insert("__floatundidf");
        cx.blacklist.insert("__floatuntisf");
        cx.blacklist.insert("__floatuntidf");
        cx.blacklist.insert("__fixsfsi");
        cx.blacklist.insert("__fixsfdi");
        cx.blacklist.insert("__fixsfti");
        cx.blacklist.insert("__fixdfsi");
        cx.blacklist.insert("__fixdfdi");
        cx.blacklist.insert("__fixdfti");
        cx.blacklist.insert("__fixunssfsi");
        cx.blacklist.insert("__fixunssfdi");
        cx.blacklist.insert("__fixunssfti");
        cx.blacklist.insert("__fixunsdfsi");
        cx.blacklist.insert("__fixunsdfdi");
        cx.blacklist.insert("__fixunsdfti");
        cx.blacklist.insert("__udivsi3");
        cx.blacklist.insert("__umodsi3");
        cx.blacklist.insert("__udivmodsi4");
        cx.blacklist.insert("__udivdi3");
        cx.blacklist.insert("__udivmoddi4");
        cx.blacklist.insert("__umoddi3");
        cx.blacklist.insert("__udivti3");
        cx.blacklist.insert("__udivmodti4");
        cx.blacklist.insert("__umodti3");
        cx.blacklist.insert("memcpy");
        cx.blacklist.insert("memmove");
        cx.blacklist.insert("memset");
        cx.blacklist.insert("memcmp");
        cx.blacklist.insert("__powisf2");
        cx.blacklist.insert("__powidf2");
        cx.blacklist.insert("__addsf3");
        cx.blacklist.insert("__adddf3");
        cx.blacklist.insert("__subsf3");
        cx.blacklist.insert("__subdf3");
        cx.blacklist.insert("__divsi3");
        cx.blacklist.insert("__divdi3");
        cx.blacklist.insert("__divti3");
        cx.blacklist.insert("__modsi3");
        cx.blacklist.insert("__moddi3");
        cx.blacklist.insert("__modti3");
        cx.blacklist.insert("__divmodsi4");
        cx.blacklist.insert("__divmoddi4");
        cx.blacklist.insert("__muldi3");
        cx.blacklist.insert("__multi3");
        cx.blacklist.insert("__mulosi4");
        cx.blacklist.insert("__mulodi4");
        cx.blacklist.insert("__muloti4");

        if let Some(section) = module.export_section() {
            for (i, entry) in section.entries().iter().enumerate() {
                cx.add_export_entry(entry, i as u32);
            }
        }
        if let Some(section) = module.data_section() {
            for entry in section.entries() {
                cx.add_data_segment(entry);
            }
        }
        if let Some(tables) = module.table_section() {
            for i in 0..tables.entries().len() as u32 {
                cx.add_table(i);
            }
        }
        if let Some(elements) = module.elements_section() {
            for seg in elements.entries() {
                cx.add_element_segment(seg);
            }
        }
        if let Some(i) = module.start_section() {
            cx.add_function(i);
        }
        cx.analysis
    };

    let cx = RemapContext::new(&module, &analysis);
    for i in (0..module.sections().len()).rev() {
        let retain = match module.sections_mut()[i] {
			Section::Unparsed { .. } => continue,
			Section::Custom(_) => continue,
			Section::Type(ref mut s) => cx.remap_type_section(s),
			Section::Import(ref mut s) => cx.remap_import_section(s),
			Section::Function(ref mut s) => cx.remap_function_section(s),
			Section::Table(ref mut s) => cx.remap_table_section(s),
			Section::Memory(ref mut s) => cx.remap_memory_section(s),
			Section::Global(ref mut s) => cx.remap_global_section(s),
			Section::Export(ref mut s) => cx.remap_export_section(s),
			Section::Start(ref mut i) => { cx.remap_function_idx(i); true }
			Section::Element(ref mut s) => cx.remap_element_section(s),
			Section::Code(ref mut s) => cx.remap_code_section(s),
			Section::Data(ref mut s) => cx.remap_data_section(s),
        };
        if !retain {
            debug!("remove empty section");
            module.sections_mut().remove(i);
        }
    }

    parity_wasm::serialize_to_file(&args[2], module).unwrap();
}

#[derive(Default)]
struct Analysis {
    functions: BTreeSet<u32>,
    codes: BTreeSet<u32>,
    tables: BTreeSet<u32>,
    memories: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    types: BTreeSet<u32>,
    imports: BTreeSet<u32>,
    exports: BTreeSet<u32>,
}

struct LiveContext<'a> {
    blacklist: HashSet<&'static str>,
    function_section: Option<&'a FunctionSection>,
    type_section: Option<&'a TypeSection>,
    code_section: Option<&'a CodeSection>,
    table_section: Option<&'a TableSection>,
    memory_section: Option<&'a MemorySection>,
    global_section: Option<&'a GlobalSection>,
    import_section: Option<&'a ImportSection>,
    analysis: Analysis,
}

impl<'a> LiveContext<'a> {
    fn new(module: &'a Module) -> LiveContext<'a> {
        LiveContext {
            blacklist: HashSet::new(),
            function_section: module.function_section(),
            type_section: module.type_section(),
            code_section: module.code_section(),
            table_section: module.table_section(),
            memory_section: module.memory_section(),
            global_section: module.global_section(),
            import_section: module.import_section(),
            analysis: Analysis::default(),
        }
    }

    fn add_function(&mut self, mut idx: u32) {
        if !self.analysis.functions.insert(idx) {
            return
        }
        if let Some(imports) = self.import_section {
            if let Some(import) = imports.entries().get(idx as usize) {
                self.analysis.imports.insert(idx);
                return self.add_import_entry(import);
            }
            idx -= imports.entries().len() as u32;
        }

        self.analysis.codes.insert(idx);
        let functions = self.function_section.expect("no functions section");
        self.add_type(functions.entries()[idx as usize].type_ref());
        let codes = self.code_section.expect("no codes section");
        self.add_func_body(&codes.bodies()[idx as usize]);
    }

    fn add_table(&mut self, idx: u32) {
        if !self.analysis.tables.insert(idx) {
            return
        }
        let tables = self.table_section.expect("no table section");
        let table = &tables.entries()[idx as usize];
        drop(table);
    }

    fn add_memory(&mut self, idx: u32) {
        if !self.analysis.memories.insert(idx) {
            return
        }
        let memories = self.memory_section.expect("no memory section");
        let memory = &memories.entries()[idx as usize];
        drop(memory);
    }

    fn add_global(&mut self, idx: u32) {
        if !self.analysis.globals.insert(idx) {
            return
        }
        let globals = self.global_section.expect("no global section");
        let global = &globals.entries()[idx as usize];
        self.add_global_type(global.global_type());
        self.add_init_expr(global.init_expr());
    }

    fn add_global_type(&mut self, t: &GlobalType) {
        self.add_value_type(&t.content_type());
    }

    fn add_init_expr(&mut self, t: &InitExpr) {
        for opcode in t.code() {
            self.add_opcode(opcode);
        }
    }

    fn add_type(&mut self, idx: u32) {
        if !self.analysis.types.insert(idx) {
            return
        }
        let types = self.type_section.expect("no types section");
        match types.types()[idx as usize] {
            Type::Function(ref f) => {
                for param in f.params() {
                    self.add_value_type(param);
                }
                if let Some(ref ret) = f.return_type() {
                    self.add_value_type(ret);
                }
            }
        }
    }

    fn add_value_type(&mut self, value: &ValueType) {
        match *value {
            ValueType::I32 => {}
            ValueType::I64 => {}
            ValueType::F32 => {}
            ValueType::F64 => {}
        }
    }

    fn add_func_body(&mut self, body: &FuncBody) {
        for local in body.locals() {
            self.add_value_type(&local.value_type());
        }
        self.add_opcodes(body.code());
    }

    fn add_opcodes(&mut self, code: &Opcodes) {
        for opcode in code.elements() {
            self.add_opcode(opcode);
        }
    }

    fn add_opcode(&mut self, code: &Opcode) {
        match *code {
            Opcode::Block(ref b) |
            Opcode::Loop(ref b) |
            Opcode::If(ref b) => self.add_block_type(b),
            Opcode::Call(f) => self.add_function(f),
            Opcode::CallIndirect(t, _) => self.add_type(t),
            Opcode::GetGlobal(i) |
            Opcode::SetGlobal(i) => self.add_global(i),
            _ => {}
        }
    }

    fn add_block_type(&mut self, bt: &BlockType) {
        match *bt {
            BlockType::Value(ref v) => self.add_value_type(v),
            BlockType::NoResult => {}
        }
    }

    fn add_export_entry(&mut self, entry: &ExportEntry, idx: u32) {
        if self.blacklist.contains(entry.field()) {
            return
        }
        self.analysis.exports.insert(idx);
        match *entry.internal() {
            Internal::Function(i) => self.add_function(i),
            Internal::Table(i) => self.add_table(i),
            Internal::Memory(i) => self.add_memory(i),
            Internal::Global(i) => self.add_global(i),
        }
    }

    fn add_import_entry(&mut self, entry: &ImportEntry) {
        match *entry.external() {
            External::Function(i) => self.add_type(i),
            External::Table(_) => {}
            External::Memory(_) => {}
            External::Global(_) => {}
        }
    }

    fn add_data_segment(&mut self, data: &DataSegment) {
        self.add_memory(data.index());
        self.add_init_expr(data.offset());
    }

    fn add_element_segment(&mut self, seg: &ElementSegment) {
        for member in seg.members() {
            self.add_function(*member);
        }
        self.add_table(seg.index());
        self.add_init_expr(seg.offset());
    }
}

struct RemapContext<'a> {
    analysis: &'a Analysis,
    functions: Vec<u32>,
    globals: Vec<u32>,
    types: Vec<u32>,
    tables: Vec<u32>,
    memories: Vec<u32>,
    nimports: u32,
}

impl<'a> RemapContext<'a> {
    fn new(m: &Module, analysis: &'a Analysis) -> RemapContext<'a> {
        fn remap(max: u32, retained: &BTreeSet<u32>) -> Vec<u32> {
            let mut v = Vec::with_capacity(max as usize);
            let mut offset = 0;
            for i in 0..max {
                if retained.contains(&i) {
                    v.push(i - offset);
                } else {
                    v.push(u32::max_value());
                    offset += 1;
                }
            }
            return v
        }

        let nfuncs = m.function_section().map(|m| m.entries().len() as u32);
        let nimports = m.import_section().map(|m| m.entries().len() as u32);
        let functions = remap(nfuncs.unwrap_or(0) + nimports.unwrap_or(0),
                              &analysis.functions);

        let nglobals = m.global_section().map(|m| m.entries().len() as u32);
        let globals = remap(nglobals.unwrap_or(0), &analysis.globals);

        let nmem = m.memory_section().map(|m| m.entries().len() as u32);
        let memories = remap(nmem.unwrap_or(0), &analysis.memories);

        let ntables = m.table_section().map(|m| m.entries().len() as u32);
        let tables = remap(ntables.unwrap_or(0), &analysis.tables);

        let ntypes = m.type_section().map(|m| m.types().len() as u32);
        let types = remap(ntypes.unwrap_or(0), &analysis.types);

        RemapContext {
            analysis,
            functions,
            globals,
            memories,
            tables,
            types,
            nimports: nimports.unwrap_or(0),
        }
    }

    fn retain<T>(&self, set: &BTreeSet<u32>, list: &mut Vec<T>, name: &str) {
        self.retain_offset(set, list, 0, name);
    }

    fn retain_offset<T>(&self,
                        set: &BTreeSet<u32>,
                        list: &mut Vec<T>,
                        offset: u32,
                        name: &str) {
        for i in (0..list.len()).rev().map(|x| x as u32) {
            if !set.contains(&(i + offset)) {
                debug!("removing {} {}", name, i + offset);
                list.remove(i as usize);
            }
        }
    }

    fn remap_type_section(&self, s: &mut TypeSection) -> bool {
        self.retain(&self.analysis.types, s.types_mut(), "type");
        for t in s.types_mut() {
            self.remap_type(t);
        }
        s.types().len() > 0
    }

    fn remap_type(&self, t: &mut Type) {
        match *t {
            Type::Function(ref mut t) => self.remap_function_type(t),
        }
    }

    fn remap_function_type(&self, t: &mut FunctionType) {
        for param in t.params_mut() {
            self.remap_value_type(param);
        }
        if let Some(m) = t.return_type_mut().as_mut() {
            self.remap_value_type(m);
        }
    }

    fn remap_value_type(&self, t: &mut ValueType) {
        drop(t);
    }

    fn remap_import_section(&self, s: &mut ImportSection) -> bool {
        self.retain(&self.analysis.imports, s.entries_mut(), "import");
        for i in s.entries_mut() {
            self.remap_import_entry(i);
        }
        s.entries().len() > 0
    }

    fn remap_import_entry(&self, s: &mut ImportEntry) {
        match *s.external_mut() {
            External::Function(ref mut f) => self.remap_function_idx(f),
            External::Table(_) => {}
            External::Memory(_) => {}
            External::Global(_) => {}
        }
    }

    fn remap_function_section(&self, s: &mut FunctionSection) -> bool {
        self.retain_offset(&self.analysis.functions,
                           s.entries_mut(),
                           self.nimports,
                           "function");
        for f in s.entries_mut() {
            self.remap_func(f);
        }
        s.entries().len() > 0
    }

    fn remap_func(&self, f: &mut Func) {
        self.remap_type_idx(f.type_ref_mut());
    }

    fn remap_table_section(&self, s: &mut TableSection) -> bool {
        self.retain(&self.analysis.tables, s.entries_mut(), "table");
        for t in s.entries_mut() {
            drop(t); // TODO
        }
        s.entries().len() > 0
    }

    fn remap_memory_section(&self, s: &mut MemorySection) -> bool {
        self.retain(&self.analysis.memories, s.entries_mut(), "memory");
        for m in s.entries_mut() {
            drop(m); // TODO
        }
        s.entries().len() > 0
    }

    fn remap_global_section(&self, s: &mut GlobalSection) -> bool {
        self.retain(&self.analysis.globals, s.entries_mut(), "global");
        for g in s.entries_mut() {
            self.remap_global_entry(g);
        }
        s.entries().len() > 0
    }

    fn remap_global_entry(&self, s: &mut GlobalEntry) {
        self.remap_global_type(s.global_type_mut());
        self.remap_init_expr(s.init_expr_mut());
    }

    fn remap_global_type(&self, s: &mut GlobalType) {
        drop(s);
    }

    fn remap_init_expr(&self, s: &mut InitExpr) {
        for code in s.code_mut() {
            self.remap_opcode(code);
        }
    }

    fn remap_export_section(&self, s: &mut ExportSection) -> bool {
        self.retain(&self.analysis.exports, s.entries_mut(), "export");
        for s in s.entries_mut() {
            self.remap_export_entry(s);
        }
        s.entries().len() > 0
    }

    fn remap_export_entry(&self, s: &mut ExportEntry) {
        match *s.internal_mut() {
            Internal::Function(ref mut i) => self.remap_function_idx(i),
            Internal::Table(ref mut i) => self.remap_table_idx(i),
            Internal::Memory(ref mut i) => self.remap_memory_idx(i),
            Internal::Global(ref mut i) => self.remap_global_idx(i),
        }
    }

    fn remap_element_section(&self, s: &mut ElementSection) -> bool {
        for s in s.entries_mut() {
            self.remap_element_segment(s);
        }
        true
    }

    fn remap_element_segment(&self, s: &mut ElementSegment) {
        let mut i = s.index();
        self.remap_table_idx(&mut i);
        assert_eq!(s.index(), i);
        for m in s.members_mut() {
            self.remap_function_idx(m);
        }
        self.remap_init_expr(s.offset_mut());
    }

    fn remap_code_section(&self, s: &mut CodeSection) -> bool {
        self.retain(&self.analysis.codes, s.bodies_mut(), "code");
        for s in s.bodies_mut() {
            self.remap_func_body(s);
        }
        s.bodies().len() > 0
    }

    fn remap_func_body(&self, b: &mut FuncBody) {
        self.remap_code(b.code_mut());
    }

    fn remap_code(&self, c: &mut Opcodes) {
        for op in c.elements_mut() {
            self.remap_opcode(op);
        }
    }

    fn remap_opcode(&self, op: &mut Opcode) {
        match *op {
            Opcode::Block(ref mut b) |
            Opcode::Loop(ref mut b) |
            Opcode::If(ref mut b) => self.remap_block_type(b),
            Opcode::Call(ref mut f) => self.remap_function_idx(f),
            Opcode::CallIndirect(ref mut t, _) => self.remap_type_idx(t),
            Opcode::GetGlobal(ref mut i) |
            Opcode::SetGlobal(ref mut i) => self.remap_global_idx(i),
            _ => {}
        }
    }

    fn remap_block_type(&self, bt: &mut BlockType) {
        match *bt {
            BlockType::Value(ref mut v) => self.remap_value_type(v),
            BlockType::NoResult => {}
        }
    }

    fn remap_data_section(&self, s: &mut DataSection) -> bool {
        for data in s.entries_mut() {
            self.remap_data_segment(data);
        }
        true
    }

    fn remap_data_segment(&self, segment: &mut DataSegment) {
        let mut i = segment.index();
        self.remap_memory_idx(&mut i);
        assert_eq!(segment.index(), i);
        self.remap_init_expr(segment.offset_mut());
    }

    fn remap_type_idx(&self, i: &mut u32) {
        *i = self.types[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_function_idx(&self, i: &mut u32) {
        *i = self.functions[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_global_idx(&self, i: &mut u32) {
        *i = self.globals[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_table_idx(&self, i: &mut u32) {
        *i = self.tables[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_memory_idx(&self, i: &mut u32) {
        *i = self.memories[*i as usize];
        assert!(*i != u32::max_value());
    }
}