use std::rc::Rc;

use crate::component::*;
use crate::orchestrator::ExecutionContext;

use petgraph::graph::NodeIndex;
use petgraph::Direction;
use tracing::trace;

#[derive(Debug)]
pub struct ComponentInstance {
  pub id: Rc<str>,
  pub node_name: String,
  pub(crate) component: Component,
  fired_nodes: Vec<NodeIndex>,
  active_nodes: Vec<NodeIndex>,
  staged_nodes: Vec<NodeIndex>,
  incoming_signals: Vec<NodeIndex>,
  instance_cycle: usize,
}

// ComponentInstance is in charge of executing it's own entire step/lifecycle with staging and active cell buffers
// rather than have that managed by a single global executor. This helps maintain locality of cells and their operands.
// It will also help identify boundaries for splitting processing across multiple threads.

impl ComponentInstance {
  pub fn new(
    node_name: String,
    component: &Component,
    init_cells: &[NodeIndex],
  ) -> ComponentInstance {
    trace!("ComponentInstance::new");
    ComponentInstance {
      id: Rc::from(cuid::cuid().unwrap()),
      node_name,
      component: component.clone(),
      fired_nodes: vec![],
      active_nodes: vec![],
      staged_nodes: init_cells.to_vec(),
      incoming_signals: vec![],
      instance_cycle: 0,
    }
  }

  pub fn is_active(&self) -> bool {
    self.staged_nodes.len() > 0 || self.fired_nodes.len() > 0 || self.incoming_signals.len() > 0
  }

  pub(crate) fn step(&mut self, context: &mut ExecutionContext) -> bool {
    self.propagate_fired_signals();
    self.stage_signaled_and_associated_nodes(context);
    if self.staged_nodes.len() > 0 {
      std::mem::swap(&mut self.active_nodes, &mut self.staged_nodes);
      self.staged_nodes.clear();
      self.process_active_nodes();
    }
    self.instance_cycle += 1;
    self.is_active()
  }

  fn propagate_fired_signals(&mut self) {
    // Set connected signal flags according to connections
    let graph = &mut self.component.graph;
    self.fired_nodes.extend_from_slice(&self.incoming_signals);
    self.incoming_signals.clear();
    for cell_index in self.fired_nodes.iter() {
      let mut edges = graph
        .neighbors_directed(*cell_index, Direction::Outgoing)
        .detach();
      while let Some((edge_index, target_index)) = edges.next(&graph) {
        let synapse = &mut graph[edge_index];
        if let Edge::Signal(signal) = synapse {
          let bit = signal.signal_bit;
          match &mut graph[target_index] {
            Node::Cell(cell) => {
              cell.set_signal(bit);
            }
            _ => {
              // no other node types should have signals
            }
          }
        }
      }
    }
  }

  fn stage_signaled_and_associated_nodes(&mut self, context: &mut ExecutionContext) {
    // Stage connected cells that are not already staged
    let graph = &mut self.component.graph;
    for node_index in self.fired_nodes.iter() {
      trace!("staging connections of {:?}", node_index);
      let mut edges = graph
        .neighbors_directed(*node_index, Direction::Outgoing)
        .detach();
      while let Some((edge, target_index)) = edges.next(&graph) {
        match &mut graph[edge] {
          Edge::Signal(Signal { signal_bit: _ }) => match &mut graph[target_index] {
            Node::Cell(cell) => {
              if !cell.flags.contains(CellFlags::STAGED) {
                trace!("staging cell {:?}", target_index);
                self.staged_nodes.push(target_index);
                cell.flags.insert(CellFlags::STAGED);
              }
            }
            Node::ConnectorOut(con) => {
              if let Some(ref instance_con_ix) = con.to_instance_connector {
                context.signal_connector(instance_con_ix.clone());
              }
            }
            _ => {
              panic!("Invalid signal receiver node {:?}", target_index);
            }
          },
          Edge::Connection(_) => {
            panic!("Invalid signal receiver node {:?}", target_index);
          }
          _ => {}
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
          cell.flags.remove(CellFlags::STAGED);
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
        _ => {
          unimplemented!("No other node types should be active");
        }
      }
    }
  }

  pub fn signal_connector_in(&mut self, node_index: NodeIndex) {
    self.incoming_signals.push(node_index);
  }
}

#[cfg(test)]
mod tests {
  use crate::component::*;
  use crate::component_instance::ComponentInstance;
  use crate::orchestrator::{ExecutionContext, OrchestratorData};

  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works() {
    let mut component = Component::new("AComponent".to_string());

    let cell_a = component.graph.add_node(Node::Cell(CellNode::one_shot()));
    let cell_b = component.graph.add_node(Node::Cell(CellNode::relay()));
    let cell_c = component.graph.add_node(Node::Cell(CellNode::relay()));
    let cell_d = component.graph.add_node(Node::Cell(CellNode::relay()));
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::Signal(Signal { signal_bit: 0 }));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::Signal(Signal { signal_bit: 0 }));
    let init_cells = [cell_a];

    let mut instance = ComponentInstance::new("root_node".to_string(), &component, &init_cells);

    let mut data = OrchestratorData::new();

    data.add_root_component(component);

    let mut context = ExecutionContext::new();

    while instance.step(&mut context) {}

    assert_eq!(instance.instance_cycle, 4);
  }
}
