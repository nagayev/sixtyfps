//! Models

/*
use super::datastructures::ComponentVTable;

/// Virtual table for a model.
///
/// TODO: how to represent the data
///
/// TODO: how to get notification when it changes
#[repr(C)]
#[vtable]
pub struct ModelVTable {
    /// Number of items
    count: unsafe fn(VRef<ModelVTable>) -> u32,

    /// Returns the data. (FIXME: find out what this returns exactly)
    data: unsafe fn(VRef<ModelVTable>, n: u32) -> *const (),
}*/
/*
/// This structure will hold a vector of the component instaces
#[repr(C)]
pub struct ComponentVecHolder {
    mode: vtable::VBox<ModelType>
    // Possible optimization: all the VBox should have the same VTable kown to the parent component
    _todo: Vec<vtable::VBox<super::datastructures::ComponentVTable>>,
}
*/

/// Component that can be instantiated by a repeater.
pub trait RepeatedComponent: crate::abi::datastructures::Component {
    /// The data corresponding to the model
    type Data;

    /// Update this component at the given index and the given data
    fn update(&self, index: usize, data: &Self::Data);
}

/// This field is put in a component when using the `for` syntax
/// It helps instantiating the components `C`
#[derive(Default)]
pub struct Repeater<C> {
    components: Vec<Box<C>>,
}

impl<Data, C> Repeater<C>
where
    C: RepeatedComponent<Data = Data>,
{
    /// Called when the model is changed
    pub fn update_model(&mut self, data: &[Data]) {
        self.components.clear();
        for (i, d) in data.iter().enumerate() {
            let c = C::create();
            c.update(i, d);
            self.components.push(Box::new(c));
        }
    }

    /// Call the visitor for each component
    pub fn visit(&self, mut visitor: super::datastructures::ItemVisitorRefMut) {
        for c in &self.components {
            c.visit_children_item(-1, visitor.borrow_mut());
        }
    }
}
