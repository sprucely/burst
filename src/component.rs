use std::cell::RefCell;
use std::hash::Hash;
use std::ops::Deref;
use std::rc::Weak;

use bitflags::bitflags;
use petgraph::graph::Graph;
use petgraph::graph::NodeIndex;

use crate::component_instance::ComponentInstance;
use crate::component_instance::ComponentInstanceId;

// TODO: may be time to use differing structures for components and component_instances
// since components are more about design-time considerations and component_instances runtime

bitflags! {
  #[derive(Default)]
  pub struct CellFlags: u32 {
    const FIRED = 1 << 0;
    const STAGED = 1 << 1;
  }
}

#[derive(Debug, Clone)]
pub enum Node<'a> {
  Cell(Cell),
  ConnectorIn(ConnectorIn),
  ConnectorOut(ConnectorOut<'a>),
  Component(ComponentInstanceRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentName(pub String);

impl Deref for ComponentName {
  type Target = String;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeName(pub String);

impl Deref for NodeName {
  type Target = String;

  fn deref(&self) -> &String {
    &self.0
  }
}

#[derive(Debug, Clone)]
pub struct NodeInstanceRef<'a> {
  pub node_name: NodeName,
  pub component_name: ComponentName,
  pub instance_name: NodeName,
  pub node_index: NodeIndex,
  pub instance: Option<Weak<RefCell<ComponentInstance<'a>>>>,
}

impl<'a> NodeInstanceRef<'a> {
  pub fn new(node_name: NodeName, component_name: ComponentName, instance_name: NodeName) -> Self {
    NodeInstanceRef {
      node_name,
      component_name,
      instance_name,
      node_index: NodeIndex::new(0),
      instance: None,
    }
  }
}

impl<'a> Eq for NodeInstanceRef<'a> {}

impl<'a> PartialEq for NodeInstanceRef<'a> {
  fn eq(&self, other: &Self) -> bool {
    self.node_name == other.node_name
      && self.component_name == other.component_name
      && self.instance_name == other.instance_name
  }
}

impl<'a> Hash for NodeInstanceRef<'a> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.node_name.hash(state);
    self.component_name.hash(state);
    self.instance_name.hash(state);
  }
}

#[derive(Debug, Clone)]
pub struct NodeRef<'a> {
  pub name: NodeName,
  pub component_name: ComponentName,
  pub node_instance_ref: Option<NodeInstanceRef<'a>>,
}

impl<'a> Hash for NodeRef<'a> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.name.hash(state);
    self.component_name.hash(state);
  }
}

impl<'a> PartialEq for NodeRef<'a> {
  fn eq(&self, other: &Self) -> bool {
    self.name == other.name && self.component_name == other.component_name
  }
}

#[derive(Debug, Clone)]
pub struct ComponentInstanceRef {
  pub component_name: ComponentName,
  pub node_name: NodeName,
  pub instance_graph_node_index: Option<NodeIndex>,
}

impl Hash for ComponentInstanceRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.component_name.hash(state);
    self.node_name.hash(state);
    self.instance_graph_node_index.hash(state);
  }
}

impl PartialEq for ComponentInstanceRef {
  fn eq(&self, other: &Self) -> bool {
    self.component_name == other.component_name
      && self.node_name == other.node_name
      && self.instance_graph_node_index == other.instance_graph_node_index
  }
}

impl Eq for ComponentInstanceRef {}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct ConnectorIn {
  pub node_name: NodeName,
  pub flags: CellFlags,
}

impl ConnectorIn {
  pub fn new(name: String) -> Self {
    ConnectorIn {
      node_name: NodeName(name),
      flags: CellFlags::empty(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct ConnectorOut<'a> {
  pub to_node_instance_ref: Option<NodeInstanceRef<'a>>,
}

impl<'a> ConnectorOut<'a> {
  pub fn new() -> ConnectorOut<'a> {
    ConnectorOut {
      to_node_instance_ref: None,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct Cell {
  pub cell_type: CellType,
  pub flags: CellFlags,
  pub signals: u32,
}

impl Cell {
  fn new(tp: CellType) -> Self {
    Self {
      cell_type: tp,
      flags: CellFlags::empty(),
      signals: 0,
    }
  }

  pub fn relay() -> Self {
    Self::new(CellType::Relay)
  }

  pub fn one_shot() -> Self {
    Self::new(CellType::OneShot)
  }

  pub fn get_type(&self) -> CellType {
    self.cell_type
  }

  pub fn set_signal(&mut self, signal_bit: u8) {
    self.signals |= 1 << signal_bit;
  }

  pub fn clear_signal(&mut self, signal_bit: u8) {
    self.signals &= !(1 << signal_bit);
  }

  pub fn get_signal(&self, signal_bit: u8) -> bool {
    self.signals & (1 << signal_bit) != 0
  }

  pub fn get_signals(&self) -> u32 {
    self.signals
  }

  pub fn clear_signals(&mut self) {
    self.signals = 0;
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellType {
  Relay,
  OneShot,
}

#[derive(Debug, Clone)]
pub struct Signal {
  pub signal_bit: u8,
}

#[derive(Debug, Clone)]
pub struct Connection {
  pub to_connector_name: NodeName,
}

#[derive(Debug, Clone)]
pub enum Edge {
  Signal(Signal),
  Association,
  Connection(Connection),
}

impl Edge {
  pub fn new_signal(signal_bit: u8) -> Self {
    Self::Signal(Signal { signal_bit })
  }

  pub fn new_association() -> Self {
    Self::Association
  }
}

pub type ComponentGraph<'a> = Graph<Node<'a>, Edge>;

#[derive(Debug, Clone)]
pub struct Component<'a> {
  pub name: ComponentName,
  pub graph: ComponentGraph<'a>,
  // cell_info_map: HashMap<String, CellInfo>,
}

impl<'a> Component<'a> {
  pub fn new(name: ComponentName) -> Self {
    Component {
      name,
      graph: Graph::new(),
      // cell_info_map: HashMap::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn it_works2() {
    let node_name = NodeName("test".to_string());

    assert_eq!(node_name.to_string(), "test");
  }

  #[test]
  fn it_works() {
    let mut component = Component::new(ComponentName("AComponent".to_string()));

    let cell_a = component
      .graph
      .add_node(Node::Cell(Cell::new(CellType::Relay)));
    let cell_b = component
      .graph
      .add_node(Node::Cell(Cell::new(CellType::Relay)));
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::new_signal(0));
  }

  #[test]
  fn parallel_quick_sort() {
    let _def = r#"
    #https://lalrpop.github.io/lalrpop/index.html
    #https://createlang.rs/intro.html
    #control flow graph https://vigusmao.github.io/manuscripts/structured_code_graphs.pdf
    #some example code at https://gist.github.com/wieslawsoltes/6592526
    
    const threshold: usize = 2;

    // can be exposed by any component that might mutate an array
    interface array_mutator {
      start: -> mut values: [u32];
      done: <-
    }

    fn swap(values: [u32], i: usize, j: usize) {
      let temp = values[i];
      values[i] = values[j];
      values[j] = temp;
    }

    fn insertion_sort(values: [u32])
    {
      for (let i = 1; i < values.len; i++) {
        let a = values[i];
        int j = i - 1;

        while (j >= 0 && values[j] > a) {
          values[j + 1] = values[j];
          j--;
        }
        values[j + 1] = a;
      }
    }

    fn partition(values: [u32]) {
      // pick a pivot element and move its value to end
      let pivot = values.len / 2;
      let pivotValue = values[pivot];
      swap(values, pivot, values.len - 1)
      pivot = 0;

      for (let i = 0; i < values.len - 1; i++) {
        if (values[i] >= pivotValue) {
          swap(values, pivot, i);
          pivot++;
        }
      }

      // move pivot value to its final location
      swap(values, pivot, values.len - 1);
      return pivot;
    }

    pub component quick_sort {
      pub con: array_mutator;

      on (con.start) {
        if (con.start.values.length <= threshold) {
          insertion_sort(con.start.values);
        }
        else {
          let pivot = partition(con.start.values);
          let quick_sort_1 = new quick_sort();
          let quick_sort_2 = new quick_sort();
          quick_sort_1.con.start(con.start.values[..pivot]);
          quick_sort_2.con.start(con.start.values[pivot + 1..]);
          // on [any|all|seq] ([...])
          on all (quick_sort_1.con.done, quick_sort_2.con.done) {
            return;
          }
        }
      }
    }
    "#;
  }
}
