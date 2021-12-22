use std::cell::RefCell;
use std::rc::Rc;

use crate::component::*;
use crate::orchestrator::ExecutionContext;

use petgraph::graph::NodeIndex;
use petgraph::Direction;
use tracing::trace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentInstanceId(String);

impl ComponentInstanceId {
  pub fn new() -> ComponentInstanceId {
    ComponentInstanceId(cuid::cuid().unwrap())
  }
}

#[derive(Debug)]
pub struct ComponentInstance {
  pub id: ComponentInstanceId,
  component: Component,
  fired_nodes: Vec<NodeIndex>,
  active_nodes: Vec<NodeIndex>,
  staged_nodes: Vec<NodeIndex>,
  instance_cycle: usize,
  execution_context: ExecutionContext,
  self_rc: Option<Rc<RefCell<ComponentInstance>>>,
}

// ComponentInstance is in charge of executing it's own entire step/lifecycle with staging and active cell buffers
// rather than have that managed by a single global executor. This helps maintain locality of cells and their operands.
// It will also help identify boundaries for splitting processing across multiple threads.

impl ComponentInstance {
  pub fn new(
    component: &Component,
    init_cells: &[NodeIndex],
    execution_context: ExecutionContext,
  ) -> Rc<RefCell<ComponentInstance>> {
    let instance = ComponentInstance {
      id: ComponentInstanceId::new(),
      component: component.clone(),
      fired_nodes: vec![],
      active_nodes: vec![],
      staged_nodes: init_cells.to_vec(),
      instance_cycle: 0,
      execution_context,
      self_rc: None,
    };
    let instance_ref = Rc::new(RefCell::new(instance));
    instance_ref.borrow_mut().self_rc = Some(instance_ref.clone());
    return instance_ref;
  }

  // pub fn weak(&self) -> Weak<RefCell<ComponentInstance>> {
  //   let Some(component_instance_rc) = self.self_rc.clone();
  //   return Rc::downgrade(&component_instance_rc);
  // }

  pub fn is_active(&self) -> bool {
    self.staged_nodes.len() > 0 || self.fired_nodes.len() > 0
  }

  pub fn run(&mut self) {
    while self.step() {}
  }

  pub fn step(&mut self) -> bool {
    self.propagate_fired_signals();
    self.stage_signaled_and_associated_nodes();
    if self.staged_nodes.len() > 0 {
      std::mem::swap(&mut self.active_nodes, &mut self.staged_nodes);
      self.staged_nodes.clear();
      self.process_active_nodes();
    }
    self.instance_cycle += 1;
    return self.fired_nodes.len() > 0;
  }

  fn propagate_fired_signals(&mut self) {
    // Set connected signal flags according to connections
    let graph = &mut self.component.graph;
    for cell_index in self.fired_nodes.iter() {
      let mut edges = graph
        .neighbors_directed(*cell_index, Direction::Outgoing)
        .detach();
      while let Some((edge_index, target_index)) = edges.next(&graph) {
        let synapse = &mut graph[edge_index];
        if let Edge::Signal(signal) = synapse {
          let bit = signal.signal_bit;
          if let Node::Cell(cell) = &mut graph[target_index] {
            cell.set_signal(bit);
          }
          // no other node types should have signals
        }
      }
    }
  }

  fn stage_signaled_and_associated_nodes(&mut self) {
    // Stage connected cells that are not already staged
    let graph = &mut self.component.graph;
    for node_index in self.fired_nodes.iter() {
      trace!("staging connections of {:?}", node_index);
      let mut edges = graph
        .neighbors_directed(*node_index, Direction::Outgoing)
        .detach();
      while let Some((edge, target_index)) = edges.next(&graph) {
        if let Edge::Signal(Signal { signal_bit: _ }) = &mut graph[edge] {
          match &mut graph[target_index] {
            Node::Cell(cell) => {
              if !cell.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_nodes.push(target_index);
                cell.flags.insert(CellFlags::STAGED);
              }
            }
            Node::ConnectorOut(connector) => {
              self
                .execution_context
                .signal_connector(connector, (&self.id).clone());
            }
            _ => {
              unimplemented!();
            }
          }
        }
      }

      if let Node::Cell(_) = &mut graph[*node_index] {
        // Associated cells (sensors) are staged separately to give explicitly signaled
        // cells a chance to modify state before doing any sensing of state changes.
        let mut edges = graph
          .neighbors_directed(*node_index, Direction::Outgoing)
          .detach();
        while let Some((edge, target_index)) = edges.next(&graph) {
          if let Edge::Association = &graph[edge] {
            if let Node::Cell(cell) = &mut graph[target_index] {
              if !cell.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_nodes.push(target_index);
                cell.flags.insert(CellFlags::STAGED);
              }
            }
          }
        }
        // no other node types should be associated
      }

      match &mut graph[*node_index] {
        Node::Cell(cell) => {
          cell.flags.remove(CellFlags::FIRED);
        }
        Node::ConnectorIn(connector) => {
          connector.flags.remove(CellFlags::FIRED);
        }
        _ => {
          unimplemented!();
        }
      }
    }
    self.fired_nodes.clear();
  }

  fn process_active_nodes(&mut self) {
    let graph = &mut self.component.graph;
    for node_index in self.active_nodes.iter() {
      match &mut graph[*node_index] {
        Node::Cell(cell) => {
          match cell.cell_type {
            CellType::Relay | CellType::OneShot => {
              cell.flags.insert(CellFlags::FIRED);
            }
          }
          if cell.flags.contains(CellFlags::FIRED) {
            self.fired_nodes.push(*node_index);
          }
          // reset cell signals for next run
          // TODO: special handling for sequence detection cells which need to hold signals across multiple cycles
          cell.signals = 0;
        }
        Node::ConnectorIn(connector) => {
          connector.flags.insert(CellFlags::FIRED);
          self.fired_nodes.push(*node_index);
        }
        _ => {
          unimplemented!("No other node types should be active");
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::rc::Rc;

  use crate::component::*;
  use crate::component_instance::ComponentInstance;
  use crate::orchestrator::{ExecutionContext, Orchestrator};

  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works() {
    let mut component = Component::new("AComponent".to_string());

    let cell_a = component.graph.add_node(Node::Cell(Cell::one_shot()));
    let cell_b = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_c = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_d = component.graph.add_node(Node::Cell(Cell::relay()));
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::Signal(Signal { signal_bit: 0 }));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::Signal(Signal { signal_bit: 0 }));
    let init_cells = [cell_a];

    let instance_rc = ComponentInstance::new(
      &component,
      &init_cells,
      ExecutionContext::new(Rc::downgrade(&Orchestrator::new())),
    );
    let mut instance = instance_rc.borrow_mut();

    instance.run();

    assert_eq!(instance.instance_cycle, 4);
  }
}
