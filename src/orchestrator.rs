use super::component::*;
use super::component_instance::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;

// TODO: Add threadpool concurrency via rayon crate (https://docs.rs/rayon/)
// exellent summary of various crates at https://www.reddit.com/r/rust/comments/djzd5t/which_asyncconcurrency_crate_to_choose_from/

// TODO: Add error handling via anyhow crate (https://docs.rs/anyhow/)
// summary of error handling at https://www.reddit.com/r/rust/comments/gqe57x/what_are_you_using_for_error_handling/
// anyhow for applications, thiserror for libraries (thiserror helps to not expose internal error handling to users)

pub struct Orchestrator {
  components: HashMap<String, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  component_instances: HashMap<String, ComponentInstance>,
  active_instance_ids: HashSet<String>,
  clock_cycle: usize,
  inactivate_ids: Vec<String>,
  cell_refs_to_stage: RefCell<Vec<ConnectionInfo>>,
}

impl Orchestrator {
  pub fn new() -> Self {
    Orchestrator {
      components: HashMap::new(),
      component_instances: HashMap::new(),
      active_instance_ids: HashSet::new(),
      clock_cycle: 0,
      inactivate_ids: Vec::new(),
      cell_refs_to_stage: RefCell::new(Vec::new()),
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
    for id in self.active_instance_ids.iter() {
      let component_instance = self.component_instances.get_mut(id).unwrap();
      let is_active = component_instance.step();
      if !is_active {
        self.inactivate_ids.push(id.clone());
      }
    }

    for id in self.inactivate_ids.iter() {
      self.active_instance_ids.remove(id);
    }
    self.inactivate_ids.clear();
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
    let mut component = Component::new("AComponent".to_string());

    let cell_a = component.graph.add_node(Cell::one_shot());
    let cell_b = component.graph.add_node(Cell::relay());
    let cell_c = component.graph.add_node(Cell::relay());
    let cell_d = component.graph.add_node(Cell::relay());
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::new_signal(0));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::new_signal(0));
    let init_cells = [cell_a];
    let instance = ComponentInstance::new(&component, &init_cells);
    let mut orchestrator = Orchestrator::new();
    orchestrator.add_component_instance(instance);
    orchestrator.run();

    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
