use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ast::{
    Binding, EffectDef, EnumDef, Function, ItemOrigin, PassMode, StructDef, Type, TypeAliasDef,
};

use super::async_lowering::{AsyncFutureInfo, InternalAsyncLoopConstructor};
use super::calls::{CallableBridgeKey, CallableBridgeSpecialization};
use super::ctfe_value::CtfeValue;
use super::handlers::DeferredHandlerTransform;
use super::hir::{
    AccessBoundary, ContinuationAdapter, EffectCallableAdapter, EnumLayout, FunctionSig,
    HirFunction, HirGlobal, ParamSig, RuntimeHandlerAction, StructLayout, Ty,
};
use super::registry::{
    ArrayTraitExtension, ConstructorTraitImplKey, FunctionInstanceInfo, FunctionInstanceKey,
    GenericInherentExtension, GenericTraitExtension, InherentMemberSet, InherentOverloadKey,
    NominalInstanceInfo, NominalInstanceKey, NominalInstanceState, ParameterLabelShape,
    PointerInherentExtension, ResolutionState, SliceInherentExtension, SliceTraitExtension,
    TraitImplInfo, TraitImplKey, TraitSchema,
};

#[derive(Default)]
pub(super) struct CollectionState {
    pub(super) functions: HashMap<String, Function>,
    pub(super) function_origins: HashMap<String, ItemOrigin>,
    pub(super) function_accesses: HashMap<String, AccessBoundary>,
    pub(super) function_templates: HashMap<String, Function>,
    pub(super) function_overloads: HashMap<String, Vec<String>>,
    pub(super) function_template_origins: HashMap<String, ItemOrigin>,
    pub(super) function_template_order: Vec<String>,
    pub(super) function_instance_names: HashMap<FunctionInstanceKey, String>,
    pub(super) function_instances: HashMap<String, FunctionInstanceInfo>,
    pub(super) function_type_substitutions: HashMap<String, HashMap<String, Type>>,
    /// Generic core primitive methods whose instances perform a checked
    /// representation-changing integer conversion.
    pub(super) integer_conversion_templates: HashMap<String, Ty>,
    /// Instantiated checked conversion method -> (source, destination).
    pub(super) integer_conversion_intrinsics: HashMap<String, (Ty, Ty)>,
    /// Signed magnitude method -> (signed source, unsigned result).
    pub(super) integer_magnitude_intrinsics: HashMap<String, (Ty, Ty)>,
    pub(super) abstract_type_parameters: HashMap<String, String>,
    pub(super) transparent_parameter_modifiers: HashSet<String>,
    pub(super) globals: HashMap<String, Binding>,
    pub(super) global_origins: HashMap<String, ItemOrigin>,
    pub(super) global_accesses: HashMap<String, AccessBoundary>,
    pub(super) struct_defs: HashMap<String, StructDef>,
    pub(super) enum_defs: HashMap<String, EnumDef>,
    pub(super) struct_templates: HashMap<String, StructDef>,
    pub(super) enum_templates: HashMap<String, EnumDef>,
    pub(super) type_aliases: HashMap<String, TypeAliasDef>,
    pub(super) struct_template_order: Vec<String>,
    pub(super) enum_template_order: Vec<String>,
    pub(super) nominal_instance_names: HashMap<NominalInstanceKey, String>,
    pub(super) nominal_instances: HashMap<String, NominalInstanceInfo>,
    pub(super) nominal_instance_states: HashMap<NominalInstanceKey, NominalInstanceState>,
    pub(super) invalid_recursive_nominals: HashSet<String>,
    pub(super) struct_layouts: HashMap<String, StructLayout>,
    pub(super) enum_layouts: HashMap<String, EnumLayout>,
    pub(super) nominal_accesses: HashMap<String, AccessBoundary>,
    pub(super) inherent_members: HashMap<String, InherentMemberSet>,
    pub(super) inherent_overload_counts: HashMap<InherentOverloadKey, usize>,
    pub(super) inherent_overloads: HashMap<InherentOverloadKey, Vec<String>>,
    pub(super) inherent_overload_shapes: HashMap<InherentOverloadKey, HashSet<ParameterLabelShape>>,
    pub(super) generic_inherent_extensions: HashMap<String, Vec<GenericInherentExtension>>,
    pub(super) pointer_inherent_extensions: Vec<PointerInherentExtension>,
    pub(super) instantiated_pointer_extensions: HashSet<String>,
    pub(super) slice_inherent_extensions: Vec<SliceInherentExtension>,
    pub(super) slice_trait_extensions: Vec<SliceTraitExtension>,
    pub(super) instantiated_slice_extensions: HashSet<String>,
    pub(super) array_trait_extensions: Vec<ArrayTraitExtension>,
    pub(super) instantiated_array_trait_extensions: HashSet<String>,
    pub(super) instantiating_array_trait_extension: usize,
    pub(super) generic_trait_extensions: HashMap<String, Vec<GenericTraitExtension>>,
    pub(super) instantiating_generic_trait_extension: usize,
    pub(super) generic_inherent_functions: HashMap<(String, String), String>,
    pub(super) suppress_generic_inherent_instantiation: usize,
    pub(super) traits: HashMap<String, TraitSchema>,
    pub(super) closed_type_values: HashMap<String, Vec<String>>,
    pub(super) effects: HashSet<String>,
    pub(super) effect_defs: HashMap<String, EffectDef>,
    pub(super) trait_impl_headers: HashSet<TraitImplKey>,
    pub(super) constructor_trait_impl_headers: HashSet<ConstructorTraitImplKey>,
    pub(super) constructor_trait_impl_methods:
        HashMap<ConstructorTraitImplKey, HashMap<String, String>>,
    pub(super) trait_impls: HashMap<TraitImplKey, TraitImplInfo>,
    pub(super) trait_methods_by_receiver: HashMap<(Ty, String), Vec<TraitImplKey>>,
    pub(super) copy_nominals: HashSet<Ty>,
    pub(super) copy_impls_finalized: bool,
    pub(super) function_order: Vec<String>,
    pub(super) global_order: Vec<String>,
    pub(super) struct_order: Vec<String>,
    pub(super) enum_order: Vec<String>,
}

#[derive(Default)]
pub(super) struct LoweringState {
    pub(super) signatures: HashMap<String, FunctionSig>,
    pub(super) global_annotations: HashMap<String, Option<Ty>>,
    pub(super) function_states: HashMap<String, ResolutionState>,
    pub(super) global_states: HashMap<String, ResolutionState>,
    pub(super) ctfe_global_values: HashMap<String, CtfeValue>,
    pub(super) ctfe_active_globals: HashSet<String>,
    pub(super) hir_functions: HashMap<String, HirFunction>,
    pub(super) lifted_functions: Vec<HirFunction>,
    pub(super) next_closure: usize,
    pub(super) callable_bridge_specializations:
        HashMap<CallableBridgeKey, CallableBridgeSpecialization>,
    pub(super) partial_parameter_shapes: HashMap<(String, usize), Vec<Vec<ParamSig>>>,
    pub(super) handler_frame_parameter_modes: HashMap<String, Vec<PassMode>>,
    pub(super) hir_globals: HashMap<String, HirGlobal>,
    pub(super) array_types: HashSet<Ty>,
    pub(super) tuple_types: HashSet<Ty>,
    pub(super) string_literals: BTreeSet<String>,
    pub(super) continuation_adapters: Vec<ContinuationAdapter>,
    pub(super) effect_callable_adapters: Vec<EffectCallableAdapter>,
    pub(super) runtime_handler_actions: HashMap<(String, usize, usize), RuntimeHandlerAction>,
    pub(super) deferred_handler_transforms: HashMap<String, DeferredHandlerTransform>,
    pub(super) async_futures: HashMap<String, AsyncFutureInfo>,
    pub(super) internal_async_loop_constructors: HashMap<String, InternalAsyncLoopConstructor>,
    pub(super) next_async_future: usize,
    pub(super) async_factory_depth: usize,
}
