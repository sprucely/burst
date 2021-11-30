use super::component::*;
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use tracing::trace;

pub struct ComponentInstance {
  pub id: String,
  component: Component,
  fired_cells: Vec<NodeIndex>,
  processing_cells: Vec<NodeIndex>,
  staging_cells: Vec<NodeIndex>,
  clock_cycle: usize,
}

// TODO: Consider optimization stragies - Cells and their operands should be grouped by component for cache locality
// Component instance could maintain it's own execution state
// - Orchestrator would need to maintain a topologically sorted (for shared parent/child data) list of component instances with staged cells
// - Component instance would need a way to change it's active status

impl ComponentInstance {
  pub fn new(component: &Component, init_cells: &[NodeIndex]) -> ComponentInstance {
    ComponentInstance {
      id: cuid::cuid().unwrap(),
      component: component.clone(),
      fired_cells: Vec::new(),
      processing_cells: Vec::new(),
      staging_cells: init_cells.to_vec(),
      clock_cycle: 0,
    }
  }

  pub fn is_active(&self) -> bool {
    self.staging_cells.len() > 0 || self.fired_cells.len() > 0
  }

  pub fn run(&mut self) {
    while self.step() {}
  }

  pub fn step(&mut self) -> bool {
    self.process_fired_cells();
    self.stage_connected_cells();
    if self.staging_cells.len() > 0 {
      std::mem::swap(&mut self.processing_cells, &mut self.staging_cells);
      self.staging_cells.clear();
      self.run_processing_cells();
    }
    self.clock_cycle += 1;
    return self.fired_cells.len() > 0;
  }

  fn process_fired_cells(&mut self) {
    // Set connected signal flags according to connections
    let graph = &mut self.component.graph;
    for cell_index in self.fired_cells.iter() {
      let mut edges = graph
        .neighbors_directed(*cell_index, Direction::Outgoing)
        .detach();
      while let Some((edge_index, target_index)) = edges.next(&graph) {
        let synapse = graph.edge_weight_mut(edge_index).unwrap();
        match *synapse {
          Synapse::Connection { signal_bit } => {
            let target = graph.node_weight_mut(target_index).unwrap();
            target.set_signal(signal_bit);
          }
          _ => {}
        }
      }
    }
  }

  fn stage_connected_cells(&mut self) {
    // Stage connected cells that are not already staged
    let graph = &mut self.component.graph;
    for cell_index in self.fired_cells.iter() {
      trace!("staging connections of {:?}", cell_index);
      let mut edges = graph
        .neighbors_directed(*cell_index, Direction::Outgoing)
        .detach();
      while let Some((_, target_index)) = edges.next(&graph) {
        let target = graph.node_weight_mut(target_index).unwrap();
        if !target.flags.contains(CellFlags::STAGED) {
          trace!("staging {:?}", target_index);
          self.staging_cells.push(target_index);
          target.flags.insert(CellFlags::STAGED);
        }
      }
      let cell = graph.node_weight_mut(*cell_index).unwrap();
      cell.flags.remove(CellFlags::FIRED);
    }
    self.fired_cells.clear();
  }

  fn run_processing_cells(&mut self) {
    let graph = &mut self.component.graph;
    for cell_index in self.processing_cells.iter() {
      let cell = graph.node_weight_mut(*cell_index).unwrap();
      trace!("running {:?}", cell_index);
      cell.run();
      if cell.flags.contains(CellFlags::FIRED) {
        self.fired_cells.push(*cell_index);
      }
      // reset cell signals for next run
      // TODO: special handling for sequence detection cells which need to hold signals across multiple cycles
      cell.signals = 0;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works() {
    let mut component = Component::new();

    let cell_a = component.graph.add_node(Cell::new(CellType::OneShot));
    let cell_b = component.graph.add_node(Cell::new(CellType::Relay));
    let cell_c = component.graph.add_node(Cell::new(CellType::Relay));
    let cell_d = component.graph.add_node(Cell::new(CellType::Relay));
    component
      .graph
      .add_edge(cell_a, cell_b, Synapse::Connection { signal_bit: 0 });
    component
      .graph
      .add_edge(cell_b, cell_c, Synapse::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Synapse::Connection { signal_bit: 0 });
    let init_cells = [cell_a];
    let mut instance = ComponentInstance::new(&component, &init_cells);
    instance.run();

    //assert!(super::grammar::TermParser::new().parse("22").is_ok());
    assert_eq!(instance.clock_cycle, 4);
  }
}
