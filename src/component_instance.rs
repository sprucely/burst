use super::component::*;
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use tracing::trace;

#[derive(Debug)]
pub struct ComponentInstance {
  pub id: String,
  component: Component,
  fired_cells: Vec<NodeIndex>,
  active_cells: Vec<NodeIndex>,
  staged_cells: Vec<NodeIndex>,
  instance_cycle: usize,
}

// ComponentInstance is in charge of executing it's own entire step/lifecycle with staging and active cell buffers
// rather than have that managed by a single global executor. This helps maintain locality of cells and their operands.
// It will also help identify boundaries for splitting processing across multiple threads.

impl ComponentInstance {
  pub fn new(component: &Component, init_cells: &[NodeIndex]) -> ComponentInstance {
    ComponentInstance {
      id: cuid::cuid().unwrap(),
      component: component.clone(),
      fired_cells: vec![],
      active_cells: vec![],
      staged_cells: init_cells.to_vec(),
      instance_cycle: 0,
    }
  }

  pub fn is_active(&self) -> bool {
    self.staged_cells.len() > 0 || self.fired_cells.len() > 0
  }

  pub fn run(&mut self) {
    while self.step() {}
  }

  pub fn step(&mut self) -> bool {
    self.propagate_fired_signals();
    self.stage_signaled_and_associated_cells();
    if self.staged_cells.len() > 0 {
      std::mem::swap(&mut self.active_cells, &mut self.staged_cells);
      self.staged_cells.clear();
      self.process_active_cells();
    }
    self.instance_cycle += 1;
    return self.fired_cells.len() > 0;
  }

  fn propagate_fired_signals(&mut self) {
    // Set connected signal flags according to connections
    let graph = &mut self.component.graph;
    for cell_index in self.fired_cells.iter() {
      let mut edges = graph
        .neighbors_directed(*cell_index, Direction::Outgoing)
        .detach();
      while let Some((edge_index, target_index)) = edges.next(&graph) {
        let synapse = &mut graph[edge_index];
        if let Edge::Signal(signal) = synapse {
          let bit = signal.signal_bit;
          let target = &mut graph[target_index];
          target.set_signal(bit);
        }
      }
    }
  }

  fn stage_signaled_and_associated_cells(&mut self) {
    // Stage connected cells that are not already staged
    let graph = &mut self.component.graph;
    for cell_index in self.fired_cells.iter() {
      trace!("staging connections of {:?}", cell_index);
      let cell = &graph[*cell_index];
      match cell.get_type() {
        _ => {
          let mut edges = graph
            .neighbors_directed(*cell_index, Direction::Outgoing)
            .detach();
          while let Some((edge, target_index)) = edges.next(&graph) {
            if let Edge::Signal(Signal { signal_bit: _ }) = &mut graph[edge] {
              let target = &mut graph[target_index];
              if !target.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_cells.push(target_index);
                target.flags.insert(CellFlags::STAGED);
              }
            }
          }

          // Associated cells (sensors) are staged separately to give explicitly signaled
          // cells a chance to modify state before doing any sensing of state changes.
          let mut edges = graph
            .neighbors_directed(*cell_index, Direction::Outgoing)
            .detach();
          while let Some((edge, target_index)) = edges.next(&graph) {
            if let Edge::Association = &graph[edge] {
              let target = &mut graph[target_index];
              if !target.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_cells.push(target_index);
                target.flags.insert(CellFlags::STAGED);
              }
            }
          }
        }
      }

      let cell = &mut graph[*cell_index];
      cell.flags.remove(CellFlags::FIRED);
    }
    self.fired_cells.clear();
  }

  fn process_active_cells(&mut self) {
    let graph = &mut self.component.graph;
    for cell_index in self.active_cells.iter() {
      let cell = &mut graph[*cell_index];
      trace!("running {:?}", cell_index);
      match cell.cell_type {
        CellType::Relay | CellType::OneShot => {
          cell.flags.insert(CellFlags::FIRED);
        } // CellType::Link => {
          //   cell.flags.insert(CellFlags::FIRED);
          // }
      }
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
    let mut component = Component::new("AComponent".to_string());

    let cell_a = component.graph.add_node(Cell::one_shot());
    let cell_b = component.graph.add_node(Cell::relay());
    let cell_c = component.graph.add_node(Cell::relay());
    let cell_d = component.graph.add_node(Cell::relay());
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::Signal(Signal { signal_bit: 0 }));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::Signal(Signal { signal_bit: 0 }));
    let init_cells = [cell_a];
    let mut instance = ComponentInstance::new(&component, &init_cells);

    instance.run();

    assert_eq!(instance.instance_cycle, 4);
  }
}
