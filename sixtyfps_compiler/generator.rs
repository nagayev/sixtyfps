/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
The module responsible for the code generation.

There is one sub module for every language
*/

use std::collections::{HashSet, VecDeque};
use std::rc::{Rc, Weak};

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{BindingExpression, Expression};
use crate::langtype::Type;
use crate::namedreference::NamedReference;
use crate::object_tree::{Component, Document, ElementRc};

#[cfg(feature = "cpp")]
mod cpp;

#[cfg(feature = "rust")]
pub mod rust;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OutputFormat {
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "rust")]
    Rust,
    Interpreter,
}

impl OutputFormat {
    pub fn guess_from_extension(path: &std::path::Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            #[cfg(feature = "cpp")]
            Some("cpp") | Some("cxx") | Some("h") | Some("hpp") => Some(Self::Cpp),
            #[cfg(feature = "rust")]
            Some("rs") => Some(Self::Rust),
            _ => None,
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "cpp")]
            "cpp" => Ok(Self::Cpp),
            #[cfg(feature = "rust")]
            "rust" => Ok(Self::Rust),
            _ => Err(format!("Unknown outpout format {}", s)),
        }
    }
}

pub fn generate(
    format: OutputFormat,
    destination: &mut impl std::io::Write,
    doc: &Document,
    diag: &mut BuildDiagnostics,
) -> std::io::Result<()> {
    #![allow(unused_variables)]
    #![allow(unreachable_code)]

    if matches!(doc.root_component.root_element.borrow().base_type, Type::Invalid | Type::Void) {
        // empty document, nothing to generate
        return Ok(());
    }

    match format {
        #[cfg(feature = "cpp")]
        OutputFormat::Cpp => {
            if let Some(output) = cpp::generate(doc, diag) {
                write!(destination, "{}", output)?;
            }
        }
        #[cfg(feature = "rust")]
        OutputFormat::Rust => {
            if let Some(output) = rust::generate(doc, diag) {
                write!(destination, "{}", output)?;
            }
        }
        OutputFormat::Interpreter => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unsupported output format: The interpreter is not a valid output format yet.",
            )); // Perhaps byte code in the future?
        }
    }
    Ok(())
}

/// Visit each item in order in which they should appear in the children tree array.
/// The parameter of the visitor are
///  1. the item
///  2. the first_children_offset,
///  3. the parent index
/// DEPRECATED: Port to build_item_tree and remove this function.
#[allow(dead_code)]
pub fn build_array_helper(component: &Component, mut visit_item: impl FnMut(&ElementRc, u32, u32)) {
    visit_item(&component.root_element, 1, 0);
    visit_children(&component.root_element, 0, 1, &mut visit_item);

    fn sub_children_count(e: &ElementRc) -> usize {
        let mut count = e.borrow().children.len();
        for i in &e.borrow().children {
            count += sub_children_count(i);
        }
        count
    }

    fn visit_children(
        item: &ElementRc,
        index: u32,
        children_offset: u32,
        visit_item: &mut impl FnMut(&ElementRc, u32, u32),
    ) {
        debug_assert_eq!(index, item.borrow().item_index.get().map(|x| *x as u32).unwrap_or(index));
        let mut offset = children_offset + item.borrow().children.len() as u32;

        for i in &item.borrow().children {
            visit_item(i, offset, index);
            offset += sub_children_count(i) as u32;
        }

        let mut offset = children_offset + item.borrow().children.len() as u32;
        let mut index = children_offset;

        for e in &item.borrow().children {
            visit_children(e, index, offset, visit_item);
            index += 1;
            offset += sub_children_count(e) as u32;
        }
    }
}

/// Visit each item in order in which they should appear in the children tree array.
/// The parameter of the visitor are
///  1. The application specific state (can be used to keep track of how to reach fields)
///  2. The current component being built (can be sub-component)
///  3. the item
///  4. the first_children_offset,
///  5. the parent index
///
/// The visit_sub_component callback is called when encountering a sub-component.
/// The parameters are:
///  1. The application specific state
///  2. The current component being built (the parent of the sub-component!)
///  3. The element used to instantiate the sub-component
///  4. The first_children_offset
#[allow(dead_code)]
pub fn build_item_tree<ComponentState>(
    root_component: &Rc<Component>,
    initial_state: ComponentState,
    mut visit_item: impl FnMut(&ComponentState, &Rc<Component>, &ElementRc, u32, u32),
    mut visit_sub_component: impl FnMut(
        &ComponentState,
        &Rc<Component>,
        &ElementRc,
        u32,
    ) -> ComponentState,
) {
    if let Some(sub_component) = root_component.root_element.borrow().sub_component() {
        assert!(root_component.root_element.borrow().children.is_empty());
        let sub_compo_state =
            visit_sub_component(&initial_state, root_component, &root_component.root_element, 1);
        build_item_tree(sub_component, sub_compo_state, visit_item, visit_sub_component)
    } else {
        visit_item(&initial_state, root_component, &root_component.root_element, 1, 0);

        visit_children(
            &initial_state,
            &root_component.root_element.borrow().children,
            &root_component,
            &root_component.root_element,
            0,
            0,
            1,
            1,
            &mut visit_item,
            &mut visit_sub_component,
        );
    }

    // Size of the element's children and grand-children,
    // needed to calculate the sub-component relative children offset indices
    fn sub_children_count(e: &ElementRc) -> usize {
        let mut count = e.borrow().children.len();
        for i in &e.borrow().children {
            count += sub_children_count(i);
        }
        count
    }

    // Size of the element's children and grand-children including
    // sub-component children, needed to allocate the correct amount of
    // index spaces for sub-components.
    fn item_sub_tree_size(e: &ElementRc) -> usize {
        let e = if let Some(sub_component) = e.borrow().sub_component() {
            sub_component.root_element.clone()
        } else {
            e.clone()
        };
        let mut count = e.borrow().children.len();
        for i in &e.borrow().children {
            count += item_sub_tree_size(i);
        }
        count
    }

    fn visit_children<ComponentState>(
        state: &ComponentState,
        children: &Vec<ElementRc>,
        component: &Rc<Component>,
        parent_item: &ElementRc,
        parent_index: u32,
        relative_parent_index: u32,
        children_offset: u32,
        relative_children_offset: u32,
        visit_item: &mut impl FnMut(&ComponentState, &Rc<Component>, &ElementRc, u32, u32),
        visit_sub_component: &mut impl FnMut(
            &ComponentState,
            &Rc<Component>,
            &ElementRc,
            u32,
        ) -> ComponentState,
    ) {
        debug_assert_eq!(
            relative_parent_index,
            parent_item.borrow().item_index.get().map(|x| *x as u32).unwrap_or(parent_index)
        );
        let mut offset = children_offset + children.len() as u32;

        let mut sub_component_states = VecDeque::new();

        for child in children.iter() {
            if let Some(sub_component) = child.borrow().sub_component() {
                let sub_component_state = visit_sub_component(state, component, child, offset);
                visit_item(
                    &sub_component_state,
                    sub_component,
                    &sub_component.root_element,
                    offset,
                    parent_index,
                );
                sub_component_states.push_back(sub_component_state);
            } else {
                visit_item(state, component, child, offset, parent_index);
            }
            offset += item_sub_tree_size(child) as u32;
        }

        let mut offset = children_offset + children.len() as u32;
        let mut relative_offset = relative_children_offset + children.len() as u32;
        let mut index = children_offset;
        let mut relative_index = relative_children_offset;

        for e in children.iter() {
            if let Some(sub_component) = e.borrow().sub_component() {
                let sub_tree_state = sub_component_states.pop_front().unwrap();
                visit_children(
                    &sub_tree_state,
                    &sub_component.root_element.borrow().children,
                    sub_component,
                    &sub_component.root_element,
                    index,
                    0,
                    offset,
                    1,
                    visit_item,
                    visit_sub_component,
                );
            } else {
                visit_children(
                    state,
                    &e.borrow().children,
                    component,
                    e,
                    index,
                    relative_index,
                    offset,
                    relative_offset,
                    visit_item,
                    visit_sub_component,
                );
            }

            index += 1;
            relative_index += 1;
            let size = item_sub_tree_size(e) as u32;
            offset += size;
            relative_offset += size;
        }
    }
}

/// Will call the `handle_property` callback for every property that needs to be initialized.
/// This function makes sure to call them in order so that if constant binding need to access
/// constant properties, these are already initialized
pub fn handle_property_bindings_init(
    component: &Rc<Component>,
    mut handle_property: impl FnMut(&ElementRc, &str, &BindingExpression),
) {
    fn handle_property_inner(
        component: &Weak<Component>,
        elem: &ElementRc,
        prop_name: &str,
        binding_expression: &BindingExpression,
        handle_property: &mut impl FnMut(&ElementRc, &str, &BindingExpression),
        processed: &mut HashSet<NamedReference>,
    ) {
        let nr = NamedReference::new(elem, prop_name);
        if processed.contains(&nr) {
            return;
        }
        processed.insert(nr);
        if binding_expression.analysis.borrow().as_ref().map_or(false, |a| a.is_const) {
            // We must first handle all dependent properties in case it is a constant property

            binding_expression.expression.visit_recursive(&mut |e| {
                if let Expression::PropertyReference(nr) = e {
                    let elem = nr.element();
                    if Weak::ptr_eq(&elem.borrow().enclosing_component, component) {
                        if let Some(be) = elem.borrow().bindings.get(nr.name()) {
                            handle_property_inner(
                                component,
                                &elem,
                                nr.name(),
                                be,
                                handle_property,
                                processed,
                            );
                        }
                    }
                }
            })
        }
        handle_property(elem, prop_name, binding_expression);
    }

    let mut processed = HashSet::new();
    crate::object_tree::recurse_elem(&component.root_element, &(), &mut |elem: &ElementRc, ()| {
        for (prop_name, binding_expression) in &elem.borrow().bindings {
            handle_property_inner(
                &Rc::downgrade(component),
                elem,
                prop_name,
                binding_expression,
                &mut handle_property,
                &mut processed,
            );
        }
    });
}
