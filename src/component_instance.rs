use super::component::*;
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use tracing::trace;

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
      fired_cells: Vec::new(),
      active_cells: Vec::new(),
      staged_cells: init_cells.to_vec(),
      instance_cycle: 0,
    }
  }

  pub fn is_active(&self) -> bool {
    self.staged_cells.len() > 0 || self.fired_cells.len() > 0
  }

  pub fn run<F: FnMut(&CellRef)>(&mut self, stage_cell_ref: &mut F) {
    while self.step(stage_cell_ref) {}
  }

  pub fn step<F: FnMut(&CellRef)>(&mut self, stage_cell_ref: &mut F) -> bool {
    self.propagate_fired_signals();
    self.stage_signaled_and_associated_cells(stage_cell_ref);
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

  fn stage_signaled_and_associated_cells<F: FnMut(&CellRef)>(&mut self, stage_cell_ref: &mut F) {
    // Stage connected cells that are not already staged
    let graph = &mut self.component.graph;
    for cell_index in self.fired_cells.iter() {
      trace!("staging connections of {:?}", cell_index);
      let cell = graph.node_weight(*cell_index).unwrap();
      match cell.get_type() {
        CellType::Link => {
          // TODO: Orchestrator should tell the linked component instance to stage the cell
          stage_cell_ref(cell.link.as_ref().unwrap())
        }
        _ => {
          let mut edges = graph
            .neighbors_directed(*cell_index, Direction::Outgoing)
            .detach();
          while let Some((edge, target_index)) = edges.next(&graph) {
            if let Synapse::Connection { .. } = graph.edge_weight(edge).unwrap() {
              let target = graph.node_weight_mut(target_index).unwrap();
              if !target.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_cells.push(target_index);
                target.flags.insert(CellFlags::STAGED);
              }
            }
          }

          // Associated cells (sensors) are staged separately to give explicitly signaled
          // cells a chance to modify state before doing any sensing of said state changes.
          let mut edges = graph
            .neighbors_directed(*cell_index, Direction::Outgoing)
            .detach();
          while let Some((edge, target_index)) = edges.next(&graph) {
            if let Synapse::Association = graph.edge_weight(edge).unwrap() {
              let target = graph.node_weight_mut(target_index).unwrap();
              if !target.flags.contains(CellFlags::STAGED) {
                trace!("staging {:?}", target_index);
                self.staged_cells.push(target_index);
                target.flags.insert(CellFlags::STAGED);
              }
            }
          }
        }
      }

      let cell = graph.node_weight_mut(*cell_index).unwrap();
      cell.flags.remove(CellFlags::FIRED);
    }
    self.fired_cells.clear();
  }

  fn process_active_cells(&mut self) {
    let graph = &mut self.component.graph;
    for cell_index in self.active_cells.iter() {
      let cell = graph.node_weight_mut(*cell_index).unwrap();
      trace!("running {:?}", cell_index);
      match cell.tp {
        CellType::Relay | CellType::OneShot => {
          cell.flags.insert(CellFlags::FIRED);
        }
        CellType::Link => {
          cell.flags.insert(CellFlags::FIRED);
        }
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
    let mut component = Component::new();

    let cell_a = component.graph.add_node(Cell::one_shot());
    let cell_b = component.graph.add_node(Cell::relay());
    let cell_c = component.graph.add_node(Cell::relay());
    let cell_d = component.graph.add_node(Cell::link(CellRef {
      to_component_id: Some(cuid::cuid().unwrap()),
      to_component_instance_id: Some(cuid::cuid().unwrap()),
      to_cell_index: Some(NodeIndex::new(0)),
    }));
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

    let mut stage_cell_ref_count = 0;
    let mut stage_cell_ref = |_: &_| {
      stage_cell_ref_count += 1;
    };
    instance.run(&mut stage_cell_ref);

    assert_eq!(instance.instance_cycle, 4);
    assert_eq!(stage_cell_ref_count, 1);
  }
}
