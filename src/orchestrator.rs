use petgraph::graph::NodeIndex;
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use petgraph::{Direction, Graph};

use crate::component::*;
use crate::component_instance::*;
use std::collections::HashMap;
use std::collections::HashSet;

// TODO: Add threadpool concurrency via rayon crate (https://docs.rs/rayon/)
// exellent summary of various crates at https://www.reddit.com/r/rust/comments/djzd5t/which_asyncconcurrency_crate_to_choose_from/

// TODO: Add error handling via anyhow crate (https://docs.rs/anyhow/)
// summary of error handling at https://www.reddit.com/r/rust/comments/gqe57x/what_are_you_using_for_error_handling/
// anyhow for applications, thiserror for libraries (thiserror helps to not expose internal error handling to users)

pub struct ExecutionContext<'a> {
  callback: Box<dyn FnMut(NodeIndex, ComponentInstanceId) + 'a>,
}

impl<'a> std::fmt::Debug for ExecutionContext<'a> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ExecutionContext")
  }
}

impl<'a> ExecutionContext<'a> {
  pub fn new(
    signal_connector: impl FnMut(NodeIndex, ComponentInstanceId) + 'a,
  ) -> ExecutionContext<'a> {
    ExecutionContext {
      callback: Box::new(signal_connector),
    }
  }

  pub fn signal_connector_out(&mut self, node_index: NodeIndex, instance_id: ComponentInstanceId) {
    (self.callback)(node_index, instance_id);
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

pub type InstanceGraph = Graph<ComponentInstanceId, InstanceConnection>;

#[derive(Debug, Clone)]
pub struct InstanceConnection {
  from_connector_index: NodeIndex,
  to_connector_index: NodeIndex,
}

#[derive(Debug)]
pub struct Orchestrator<'a> {
  components: HashMap<ComponentName, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  instance_ids_to_instances: HashMap<ComponentInstanceId, ComponentInstance<'a>>,
  node_instance_refs_to_owner_ids: HashMap<NodeInstanceRef, ComponentInstanceId>,
  active_instance_ids: HashSet<ComponentInstanceId>,
  clock_cycle: usize,
  inactivate_ids: Vec<ComponentInstanceId>,
  // keep track of all connections between component instances
  connections: InstanceGraph,
  root_component_name: Option<ComponentName>,
  root_instance_id: Option<ComponentInstanceId>,
}

impl<'a> Orchestrator<'a> {
  // pub fn new(orchestrator_data: &'a mut OrchestratorData<'a>) -> Self {
  //   orchestrator_data.as_orchestrator()
  // }

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

  pub fn add_component(&mut self, component: Component) {
    self.components.insert(component.name.clone(), component);
  }

  pub fn add_root_component(&mut self, component: Component) -> &mut Self {
    self.root_component_name = Some(component.name.clone());
    self.components.insert(component.name.clone(), component);
    self
  }

  pub fn run(&mut self) -> &mut Self {
    self.active_instance_ids.clear();
    for (id, component_instance) in self.instance_ids_to_instances.iter() {
      if component_instance.is_active() {
        self.active_instance_ids.insert(id.clone());
      }
    }

    for _ in Stepper::new(self) {}

    self
  }

  pub fn step(&mut self) -> bool {
    for id in self.active_instance_ids.iter() {
      let component_instance = self.instance_ids_to_instances.get_mut(id).unwrap();
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

    self.active_instance_ids.len() > 0
  }

  // fn instantiate_component(&mut self, id: &String) -> ComponentInstance {
  //   let component = self.components[id];
  //   let mut component_instance = ComponentInstance::new(component);
  //   component_instance
  // }

  // fn resolve_connected_instance(
  //   &'a mut self,
  //   connector_out: &mut ConnectorOut,
  //   connector_out_owner_instance_id: Option<ComponentInstanceId>,
  //   connector_out_index: NodeIndex,
  // ) -> Option<(Rc<RefCell<ComponentInstance>>, NodeIndex)> {
  //   match connector_out.to_node_instance_ref {
  //     Some(ref mut to_node_instance_ref) => match self.resolve_instance(to_node_instance_ref) {
  //       Some(to_instance_rc) => {
  //         return Some((to_instance_rc.clone(), to_node_instance_ref.node_index));
  //       }
  //       None => {
  //         todo!(
  //           "improve error handling: resolve_connected_instance: to_node_instance_ref not found {:?}",
  //           to_node_instance_ref
  //         );
  //       }
  //     },
  //     None => {
  //       // todo: rethink use of graph for managing instances and connections
  //       /*
  //       ComponentInstanceId is a unique single hashable value that can allow quick lookup of instances
  //       If instances are stored in a graph, then NodeIndex can be used to lookup instances, but it would
  //       not remain stable without using the less performant StableGraph.

  //        */
  //       // let node_instance_ref = NodeInstanceRef::new(
  //       //   connector_out_owner_instance_id,
  //       //   connector_out_index,
  //       //   connector_out.connector_index,
  //       // );
  //       todo!();
  //     }
  //   }
  // }

  // fn resolve_instance(
  //   &mut self,
  //   node_instance_ref: &mut NodeInstanceRef,
  // ) -> Option<&mut ComponentInstance<'a>> {
  //   match &node_instance_ref.instance_id {
  //     Some(instance_id) => {
  //       // instance has previously been resolved for this ref
  //       return Some(self.instance_ids_to_instances.get_mut(instance_id).unwrap());
  //     }
  //     None => {
  //       match self.node_instance_refs_to_owner_ids.get(&node_instance_ref) {
  //         Some(instance_id) => {
  //           // instance exists, but has not been resolved for this ref
  //           let instance = self.instance_ids_to_instances.get_mut(instance_id).unwrap();
  //           node_instance_ref.instance_id = Some(instance_id.clone());
  //           return Some(instance);
  //         }
  //         None => {
  //           // instance does not exist, so must be created
  //           let component = self
  //             .components
  //             .get(&node_instance_ref.component_name)
  //             .unwrap();
  //           let component_instance = ComponentInstance::new(
  //             node_instance_ref.node_name.clone(),
  //             &component,
  //             &[],
  //             ExecutionContext::new({
  //               //
  //               move |node_index, instance_id| {}
  //             }),
  //           );
  //           let component_instance_id = component_instance.id.clone();
  //           node_instance_ref.instance_id = Some(component_instance_id.clone());
  //           self
  //             .instance_ids_to_instances
  //             .insert(component_instance.id.clone(), component_instance);
  //           self
  //             .node_instance_refs_to_owner_ids
  //             .insert(node_instance_ref.clone(), component_instance_id.clone());
  //           return Some(
  //             self
  //               .instance_ids_to_instances
  //               .get_mut(&component_instance_id)
  //               .unwrap(),
  //           );
  //         }
  //       }
  //     }
  //   }
  // }

  pub fn signal_connector_in(&mut self, node_index: NodeIndex) -> &mut Self {
    match self.root_instance_id {
      Some(ref root_instance_id) => {
        let root_instance_id = root_instance_id.clone();
        self.signal_connector_in_on_instance(root_instance_id, node_index)
      }
      None => match self.root_component_name {
        Some(ref root_component_name) => {
          let root_component = self.components.get(root_component_name).unwrap();
          let root_component_instance = ComponentInstance::new(
            NodeName("root".to_string()),
            root_component,
            &[],
            ExecutionContext::new({
              //
              |node_index, instance_id| {
                //
                // self.signal_connector_out(node_index, instance_id);
              }
            }),
          );
          let root_component_instance_id = root_component_instance.id.clone();
          self.root_instance_id = Some(root_component_instance_id.clone());
          self
            .instance_ids_to_instances
            .insert(root_component_instance.id.clone(), root_component_instance);
          self.signal_connector_in_on_instance(root_component_instance_id, node_index)
        }
        None => {
          todo!("Improve error handling: no root component");
        }
      },
    }

    self
  }

  pub fn signal_connector_in_on_instance(
    &mut self,
    instance_id: ComponentInstanceId,
    node_index: NodeIndex,
  ) {
    let instance = self
      .instance_ids_to_instances
      .get_mut(&instance_id)
      .unwrap();
    instance.signal_connector_in(node_index);
  }

  fn signal_connector_out(&mut self, node_index: NodeIndex, from_instance_id: ComponentInstanceId) {
    if let Some((from_instance_node_index, _)) = self
      .connections
      .node_references()
      .find(|&(_, id)| id == &from_instance_id)
    {
      if let Some(edge) = self
        .connections
        .edges_directed(from_instance_node_index, Direction::Outgoing)
        .find(|edge| edge.weight().from_connector_index == node_index)
      {
        let to_instance_node_index = edge.weight().to_connector_index;
        let to_instance_id = self.connections[edge.target()].clone();
        self.signal_connector_in_on_instance(to_instance_id, to_instance_node_index)
      }
    }
  }
}

pub struct StepStatus {}

pub struct Stepper<'a, 'b: 'a> {
  orchestrator: &'a mut Orchestrator<'b>,
}

impl<'a, 'b: 'a> Stepper<'a, 'b> {
  pub fn new(orchestrator: &'a mut Orchestrator<'b>) -> Self {
    Self { orchestrator }
  }
}

impl<'a, 'b: 'a> Iterator for Stepper<'a, 'b> {
  type Item = StepStatus;

  fn next(&mut self) -> Option<Self::Item> {
    if self.orchestrator.step() {
      Some(StepStatus {})
    } else {
      None
    }
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
  fn it_works<'a>() {
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

    //let mut data = OrchestratorData::new();
    let mut orchestrator = Orchestrator::new();
    let orchestrator = orchestrator.add_root_component(component);
    let orchestrator = orchestrator.signal_connector_in(connector_in);
    let orchestrator = orchestrator.run();

    //let stepper = Stepper::new(&mut orchestrator);

    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
