use petgraph::graph::NodeIndex;
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

pub struct ExecutionContext<'a> {
  callback: Box<dyn FnMut(&mut SignalConnectorOptions) + 'a>,
}

impl<'a> std::fmt::Debug for ExecutionContext<'a> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ExecutionContext")
  }
}

impl<'a> ExecutionContext<'a> {
  pub fn new(
    signal_connector: impl FnMut(&mut SignalConnectorOptions) + 'a,
  ) -> ExecutionContext<'a> {
    ExecutionContext {
      callback: Box::new(signal_connector),
    }
  }

  pub fn signal_connector(&mut self, options: &mut SignalConnectorOptions) {
    (self.callback)(options);
    // self
    //   .orchestrator
    //   .upgrade()
    //   .unwrap()
    //   .borrow_mut()
    //   .signal_connector(options);
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

pub type InstanceGraph = Graph<ComponentInstanceRef, InstanceConnection>;

#[derive(Debug, Clone)]
pub struct InstanceConnection {
  from_connector_index: NodeIndex,
  to_connector_index: NodeIndex,
}

#[derive(Debug)]
pub struct Orchestrator<'a> {
  components: HashMap<ComponentName, Component<'a>>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  instance_ids_to_instances: HashMap<ComponentInstanceId, Rc<RefCell<ComponentInstance<'a>>>>,
  node_instance_refs_to_owner_ids: HashMap<NodeInstanceRef<'a>, ComponentInstanceId>,
  active_instance_ids: HashSet<ComponentInstanceId>,
  clock_cycle: usize,
  inactivate_ids: Vec<ComponentInstanceId>,
  // keep track of all connections between component instances
  connections: InstanceGraph,
  root_component_name: Option<ComponentName>,
  root_instance_id: Option<ComponentInstanceId>,
}

impl<'a> Orchestrator<'a> {
  pub fn new() -> Self {
    Orchestrator {
      components: HashMap::new(),
      instance_ids_to_instances: HashMap::new(),
      node_instance_refs_to_owner_ids: HashMap::new(),
      active_instance_ids: HashSet::new(),
      clock_cycle: 0,
      inactivate_ids: Vec::new(),
      connections: Graph::new(),
      root_component_name: None,
      root_instance_id: None,
    }
  }

  // pub fn add_component_instance(&mut self, component_instance: Rc<RefCell<ComponentInstance>>) {
  //   self.instance_ids_to_instances.insert(
  //     component_instance.borrow().id.clone(),
  //     component_instance.clone(),
  //   );
  // }

  pub fn add_component(&mut self, component: Component<'a>) {
    self.components.insert(component.name.clone(), component);
  }

  pub fn add_root_component(&mut self, component: Component<'a>) {
    self.root_component_name = Some(component.name.clone());
    self.add_component(component);
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

  fn resolve_connected_instance(
    &mut self,
    connector_out: &mut ConnectorOut<'a>,
    connector_out_owner_instance_id: Option<ComponentInstanceId>,
    connector_out_index: NodeIndex,
  ) -> Option<(Rc<RefCell<ComponentInstance<'a>>>, NodeIndex)> {
    match connector_out.to_node_instance_ref {
      Some(ref mut to_node_instance_ref) => match self.resolve_instance(to_node_instance_ref) {
        Some(to_instance_rc) => {
          return Some((to_instance_rc.clone(), to_node_instance_ref.node_index));
        }
        None => {
          todo!(
            "improve error handling: resolve_connected_instance: to_node_instance_ref not found {:?}",
            to_node_instance_ref
          );
        }
      },
      None => {
        // todo: rethink use of graph for managing instances and connections
        /*
        ComponentInstanceId is a unique single hashable value that can allow quick lookup of instances
        If instances are stored in a graph, then NodeIndex can be used to lookup instances, but it would
        not remain stable without using the less performant StableGraph.

         */

        // let node_instance_ref = NodeInstanceRef::new(
        //   connector_out_owner_instance_id,
        //   connector_out_index,
        //   connector_out.connector_index,
        // );
        todo!();
      }
    }
  }

  fn resolve_instance(
    &mut self,
    node_instance_ref: &mut NodeInstanceRef<'a>,
  ) -> Option<Rc<RefCell<ComponentInstance<'a>>>> {
    match &node_instance_ref.instance {
      Some(instance) => {
        // instance has previously been resolved for this ref
        return Some(instance.upgrade().unwrap());
      }
      None => {
        match self.node_instance_refs_to_owner_ids.get(&node_instance_ref) {
          Some(instance_id) => {
            // instance exists, but has not been resolved for this ref
            let instance = self.instance_ids_to_instances.get(instance_id).unwrap();
            node_instance_ref.instance = Some(Rc::downgrade(instance));
            return Some(instance.clone());
          }
          None => {
            // instance does not exist, so must be created
            let component = self
              .components
              .get(&node_instance_ref.component_name)
              .unwrap();
            let component_instance_rc = ComponentInstance::new(
              node_instance_ref.node_name.clone(),
              &component,
              &[],
              ExecutionContext::new(|options| todo!()),
            );
            let component_instance = component_instance_rc.borrow_mut();
            node_instance_ref.instance = Some(Rc::downgrade(&component_instance_rc));
            self
              .instance_ids_to_instances
              .insert(component_instance.id.clone(), component_instance_rc.clone());
            self
              .node_instance_refs_to_owner_ids
              .insert(node_instance_ref.clone(), component_instance.id.clone());
            return Some(component_instance_rc.clone());
          }
        }
      }
    }
  }

  pub fn signal_connector(&mut self, options: &mut SignalConnectorOptions) {
    match options {
      SignalConnectorOptions::ConnectorInIndex(node_index) => self.signal_connector_in(node_index),
      SignalConnectorOptions::ConnectorInIndexForInstanceId(node_index, instance_id) => {}
      SignalConnectorOptions::ConnectorOutIndexForInstanceId(node_index, instance_id) => {}
    }
  }

  fn signal_connector_in(&mut self, node_index: &mut NodeIndex) {
    let mut instance_id: ComponentInstanceId;

    match self.root_instance_id {
      Some(ref root_instance_id) => {
        self.signal_connector_in_on_instance(&mut root_instance_id.clone(), node_index)
      }
      None => match self.root_component_name {
        Some(ref root_component_name) => {
          let root_component = self.components.get(root_component_name).unwrap();
          let root_component_instance_rc = ComponentInstance::new(
            NodeName("root".to_string()),
            root_component,
            &[],
            ExecutionContext::new(|options| todo!()),
          );
          let root_component_instance = root_component_instance_rc.borrow_mut();
          self.root_instance_id = Some(root_component_instance.id.clone());
          self.instance_ids_to_instances.insert(
            root_component_instance.id.clone(),
            root_component_instance_rc.clone(),
          );
          self.signal_connector_in_on_instance(&mut root_component_instance.id.clone(), node_index)
        }
        None => {
          todo!("Improve error handling: no root component");
        }
      },
    }
  }

  fn signal_connector_in_on_instance(
    &mut self,
    instance_id: &mut ComponentInstanceId,
    node_index: &mut NodeIndex,
  ) {
    let instance = self.instance_ids_to_instances.get(instance_id).unwrap();
    let mut instance = instance.borrow_mut();
    instance.signal_connector_in(*node_index);
  }
}

#[derive(Debug, Clone)]
pub enum SignalConnectorOptions {
  ConnectorInIndex(NodeIndex),
  ConnectorInIndexForInstanceId(NodeIndex, ComponentInstanceId),
  ConnectorOutIndexForInstanceId(NodeIndex, ComponentInstanceId),
}

#[cfg(test)]
mod tests {
  use super::*;
  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works() {
    let mut component = Component::new(ComponentName("AComponent".to_string()));

    let connector_in = component.graph.add_node(Node::ConnectorIn(ConnectorIn::new(
      "connector_in".to_string(),
    )));
    let cell_b = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_c = component.graph.add_node(Node::Cell(Cell::relay()));
    let cell_d = component.graph.add_node(Node::Cell(Cell::relay()));
    component
      .graph
      .add_edge(connector_in, cell_b, Edge::new_signal(0));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::new_signal(0));
    let mut orchestrator = Orchestrator::new();
    orchestrator.add_root_component(component);
    orchestrator.signal_connector(&mut SignalConnectorOptions::ConnectorInIndex(connector_in));
    orchestrator.run();

    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
