use super::component::*;
use super::component_instance::*;
use std::collections::HashMap;
use std::collections::HashSet;

pub struct Orchestrator {
  components: HashMap<String, Component>,
  component_instances: HashMap<String, ComponentInstance>,
  active_instance_ids: HashSet<String>,
  clock_cycle: usize,
}

impl Orchestrator {
  pub fn new() -> Self {
    Orchestrator {
      components: HashMap::new(),
      component_instances: HashMap::new(),
      active_instance_ids: HashSet::new(),
      clock_cycle: 0,
    }
  }

  pub fn add_component_instance(&mut self, component_instance: ComponentInstance) {
    self
      .component_instances
      .insert(component_instance.id.clone(), component_instance);
  }

  pub fn run(&mut self) {
    self.active_instance_ids.clear();
    for (id, component_instance) in self.component_instances.iter() {
      if component_instance.is_active() {
        self.active_instance_ids.insert(id.clone());
      }
    }

    while self.active_instance_ids.len() > 0 {
      self.step();
    }
  }

  pub fn step(&mut self) {
    let mut inactivate_ids = Vec::<String>::new();
    for id in self.active_instance_ids.iter() {
      let component_instance = self.component_instances.get_mut(id).unwrap();
      let is_active = component_instance.step();
      if !is_active {
        inactivate_ids.push(id.clone());
      }
    }

    for id in inactivate_ids {
      self.active_instance_ids.remove(&id);
    }
    self.clock_cycle += 1;
  }

  // fn instantiate_component(&mut self, id: &String) -> ComponentInstance {
  //   let component = self.components[id];
  //   let mut component_instance = ComponentInstance::new(component);
  //   component_instance
  // }
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
    let instance = ComponentInstance::new(&component, &init_cells);
    let mut orchestrator = Orchestrator::new();
    orchestrator.add_component_instance(instance);
    orchestrator.run();

    //assert!(super::grammar::TermParser::new().parse("22").is_ok());
    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
