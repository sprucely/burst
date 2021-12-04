use bitflags::bitflags;
use petgraph::graph::Graph;
use petgraph::graph::NodeIndex;

/*
ComponentGraph
  nodes:
    cells:
      effectors:
        ...
      sensors:
        ...
      sensor-effectors:
        relay
        ...
    variables
    component-references
    connector-bundles
  weights:
    synapses
      from: cell
      to: cell
    associations:
      from: effector
      to: sensor
    connections
      from: connector
      to: connector-or-reference
    operands:
      from: cell
      to: variable


Connector


*/

bitflags! {
  #[derive(Default)]
  pub struct CellFlags: u32 {
    const FIRED = 1 << 0;
    const STAGED = 1 << 1;
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellRef {
  pub to_component_id: Option<String>,
  pub to_component_instance_id: Option<String>,
  pub to_cell_index: Option<NodeIndex>,
  // TODO: re-thinking how to reference other component nodes
  // TODO: may be time to use differing structures for components and component_instances
  // since components are more about design-time considerations and component_instances runtime
  // pub to_component_name: String
  // pub to_cell_name: String
  // // The following fields are determined on-demand
  // pub to_cell_index: Option<NodeIndex>
  // pub to_component_instance_rc: Option<Rc<RefCell<ComponentInstance>>>
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cell {
  pub tp: CellType,
  pub flags: CellFlags,
  pub signals: u32,
  // references cell in other component instance
  pub link: Option<CellRef>,
}

impl Cell {
  fn new(tp: CellType) -> Self {
    Self {
      tp: tp,
      flags: CellFlags::empty(),
      signals: 0,
      link: None,
    }
  }

  pub fn relay() -> Self {
    Self::new(CellType::Relay)
  }

  pub fn one_shot() -> Self {
    Self::new(CellType::OneShot)
  }

  pub fn link(cell_ref: CellRef) -> Self {
    let mut cell = Self::new(CellType::Link);
    cell.link = Some(cell_ref);
    return cell;
  }

  pub fn get_type(&self) -> CellType {
    self.tp
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
  Link,
}

// #[derive(Debug, Clone)]
// pub struct CellInfo {
//   pub name: String,
//   pub index: NodeIndex,
// }

#[derive(Debug, Clone, Copy)]
pub enum Synapse {
  Connection { signal_bit: u8 },
  Association,
}

type ComponentGraph = Graph<Cell, Synapse>;

#[derive(Debug, Clone)]
pub struct Component {
  pub id: String,
  pub graph: ComponentGraph,
  // cell_info_map: HashMap<String, CellInfo>,
}

impl Component {
  pub fn new() -> Self {
    Component {
      id: cuid::cuid().unwrap(),
      graph: Graph::new(),
      // cell_info_map: HashMap::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn it_works() {
    let mut component = Component::new();

    let cell_a = component.graph.add_node(Cell::new(CellType::Relay));
    let cell_b = component.graph.add_node(Cell::new(CellType::Relay));
    component
      .graph
      .add_edge(cell_a, cell_b, Synapse::Connection { signal_bit: 0 });
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