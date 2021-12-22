use petgraph::Graph;

use crate::component::*;
use crate::component_instance::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;
use std::rc::Weak;

// TODO: Add threadpool concurrency via rayon crate (https://docs.rs/rayon/)
// exellent summary of various crates at https://www.reddit.com/r/rust/comments/djzd5t/which_asyncconcurrency_crate_to_choose_from/

// TODO: Add error handling via anyhow crate (https://docs.rs/anyhow/)
// summary of error handling at https://www.reddit.com/r/rust/comments/gqe57x/what_are_you_using_for_error_handling/
// anyhow for applications, thiserror for libraries (thiserror helps to not expose internal error handling to users)

#[derive(Debug, Clone)]
pub struct ExecutionContext {
  orchestrator: Weak<RefCell<Orchestrator>>,
}

impl ExecutionContext {
  pub fn new(orchestrator: Weak<RefCell<Orchestrator>>) -> ExecutionContext {
    ExecutionContext { orchestrator }
  }

  pub fn signal_connector(
    &self,
    connector: &mut ConnectorOut,
    owning_instance_id: ComponentInstanceId,
  ) {
    self
      .orchestrator
      .upgrade()
      .unwrap()
      .borrow_mut()
      .signal_connector(connector, Some(owning_instance_id));
  }
}

/// connector connection connector
/// ------<in to----from out<-----
/// ----->out from----to in>------

// #[derive(Debug, Clone, PartialEq, Hash)]
// struct Connection {
//   pub owning_instance_ref: ComponentInstanceRef,
//   pub from_connector_ref: NodeRef,
//   pub to_connector_ref: NodeRef,
// }

pub type InstanceGraph = Graph<NodeRef, InstanceConnection>;

#[derive(Debug, Clone)]
pub struct InstanceConnection;

#[derive(Debug)]
pub struct Orchestrator {
  components: HashMap<String, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  instance_ids_to_instances: HashMap<ComponentInstanceId, Rc<RefCell<ComponentInstance>>>,
  instance_refs_to_ids: HashMap<ComponentInstanceRef, ComponentInstanceId>,
  active_instance_ids: HashSet<ComponentInstanceId>,
  clock_cycle: usize,
  inactivate_ids: Vec<ComponentInstanceId>,
  self_rc: Option<Rc<RefCell<Orchestrator>>>,
  // keep track of all connections between component instances
  connections: InstanceGraph,
}

impl Drop for Orchestrator {
  fn drop(&mut self) {
    // todo: test if this is really necessary
    self.self_rc = None;
  }
}

impl Orchestrator {
  pub fn new() -> Rc<RefCell<Self>> {
    let orchestrator = Orchestrator {
      components: HashMap::new(),
      instance_ids_to_instances: HashMap::new(),
      instance_refs_to_ids: HashMap::new(),
      active_instance_ids: HashSet::new(),
      clock_cycle: 0,
      inactivate_ids: Vec::new(),
      self_rc: None,
      connections: Graph::new(),
    };
    let orchestrator_ref = Rc::new(RefCell::new(orchestrator));
    orchestrator_ref.borrow_mut().self_rc = Some(orchestrator_ref.clone());
    return orchestrator_ref;
  }

  // pub fn weak(&self) -> Weak<RefCell<Orchestrator>> {
  //   let Some(orchestrator_rc) = self.self_rc.clone();
  //   return Rc::downgrade(&orchestrator_rc);
  // }

  pub fn add_component_instance(&mut self, component_instance: Rc<RefCell<ComponentInstance>>) {
    self.instance_ids_to_instances.insert(
      component_instance.borrow().id.clone(),
      component_instance.clone(),
    );
  }

  pub fn run(&mut self) {
    self.active_instance_ids.clear();
    for (id, component_instance) in self.instance_ids_to_instances.iter() {
      if component_instance.borrow().is_active() {
        self.active_instance_ids.insert(id.clone());
      }
    }

    while self.active_instance_ids.len() > 0 {
      self.step();
    }
  }

  pub fn step(&mut self) {
    for id in self.active_instance_ids.iter() {
      let component_instance = self.instance_ids_to_instances.get(id).unwrap();
      let is_active = component_instance.borrow_mut().step();
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

  fn resolve_instance(
    &mut self,
    instance_ref: &mut ComponentInstanceRef,
    owning_instance_id: Option<ComponentInstanceId>,
  ) -> Option<Rc<RefCell<ComponentInstance>>> {
    match &instance_ref.instance {
      Some(instance) => {
        // instance has previously been resolved for this ref
        return Some(instance.upgrade().unwrap());
      }
      None => {
        match self.instance_refs_to_ids.get(&instance_ref) {
          Some(instance_id) => {
            // instance exists, but has not been resolved for this ref
            let instance = self.instance_ids_to_instances.get(instance_id).unwrap();
            instance_ref.instance = Some(Rc::downgrade(instance));
            return Some(instance.clone());
          }
          None => {
            // instance does not exist, so must be created
            let component = self.components.get(&instance_ref.component_name).unwrap();
            let component_instance_rc = ComponentInstance::new(
              &component,
              &[],
              ExecutionContext::new(Rc::downgrade(self.self_rc.as_ref().unwrap())),
            );
            let component_instance = component_instance_rc.borrow_mut();
            instance_ref.instance = Some(Rc::downgrade(&component_instance_rc));
            instance_ref.owning_instance_id = owning_instance_id;
            self
              .instance_ids_to_instances
              .insert(component_instance.id.clone(), component_instance_rc.clone());
            self
              .instance_refs_to_ids
              .insert(instance_ref.clone(), component_instance.id.clone());
            return Some(component_instance_rc.clone());
          }
        }
      }
    }
  }

  fn signal_connector(
    &mut self,
    connector: &mut ConnectorOut,
    owning_instance_id: Option<ComponentInstanceId>,
  ) {
    match self.resolve_instance(connector.instance_ref(), owning_instance_id) {
      Some(_instance) => {
        todo!();
        //instance.signal_connector(connector);
      }
      None => {
        todo!("Add error handling")
      }
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

    let cell_a = component.graph.add_node(Node::Cell(Cell::one_shot()));
    let cell_b = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_c = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_d = component.graph.add_node(Node::Cell(Cell::relay()));
    component
      .graph
      .add_edge(cell_a, cell_b, Edge::new_signal(0));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::new_signal(0));
    let init_cells = [cell_a];
    let orchestrator = Orchestrator::new();
    let instance = ComponentInstance::new(
      &component,
      &init_cells,
      ExecutionContext::new(Rc::downgrade(&orchestrator)),
    );
    let mut orchestrator = orchestrator.borrow_mut();
    orchestrator.add_component_instance(instance);
    orchestrator.run();

    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
