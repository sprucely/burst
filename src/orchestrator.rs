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
pub struct SignalRequest {
  pub instance_id: ComponentInstanceId,
  pub node_index: NodeIndex,
}

#[derive(Debug)]
pub struct Context {
  signal_requests: Vec<SignalRequest>,
}

impl Context {
  pub fn new() -> Context {
    Context {
      signal_requests: vec![],
    }
  }

  pub(crate) fn signal_connector_out(
    &mut self,
    node_index: NodeIndex,
    instance_id: ComponentInstanceId,
  ) {
    self.signal_requests.push(SignalRequest {
      instance_id,
      node_index,
    });
  }
}
#[derive(Debug)]
pub struct OrchestratorData {
  components: HashMap<ComponentName, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  node_instance_refs_to_owner_ids: HashMap<NodeInstanceRef, ComponentInstanceId>,
  active_instance_ids: HashSet<ComponentInstanceId>,
  clock_cycle: usize,
  inactivate_ids: Vec<ComponentInstanceId>,
  // keep track of all connections between component instances
  pub(crate) connections: InstanceGraph,
  root_component_name: Option<ComponentName>,
  root_instance_id: Option<ComponentInstanceId>,
  pub(crate) instance_ids_to_instances: HashMap<ComponentInstanceId, ComponentInstance>,
}

impl OrchestratorData {
  pub fn new() -> OrchestratorData {
    OrchestratorData {
      components: HashMap::new(),
      node_instance_refs_to_owner_ids: HashMap::new(),
      active_instance_ids: HashSet::new(),
      clock_cycle: 0,
      inactivate_ids: Vec::new(),
      connections: Graph::new(),
      root_component_name: None,
      root_instance_id: None,
      instance_ids_to_instances: HashMap::new(),
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
}

#[derive(Debug)]
pub struct Orchestrator<'a> {
  data: &'a mut OrchestratorData,
}

impl<'a> Orchestrator<'a> {
  // pub fn new(orchestrator_data: &'a mut OrchestratorData) -> Self {
  //   orchestrator_data.as_orchestrator()
  // }

  pub fn new(data: &'a mut OrchestratorData) -> Self {
    Orchestrator { data }
  }

  pub fn add_component(&mut self, component: Component) {
    self.data.add_component(component);
  }

  pub fn add_root_component(&mut self, component: Component) -> &mut Self {
    self.data.add_root_component(component);
    self
  }

  pub fn run(&mut self) -> &mut Self {
    self.data.active_instance_ids.clear();
    for (id, component_instance) in self.data.instance_ids_to_instances.iter() {
      if component_instance.is_active() {
        self.data.active_instance_ids.insert(id.clone());
      }
    }

    let stepper = Stepper {
      data: &mut self.data,
    };
    for _tmp in stepper {}
    //while self.step(&mut self.data.active_instance_ids) {}

    self
  }

  fn step(
    active_instance_ids: &mut HashSet<ComponentInstanceId>,
    instance_ids_to_instances: &mut HashMap<ComponentInstanceId, ComponentInstance>,
    inactivate_ids: &mut Vec<ComponentInstanceId>,
    clock_cycle: &mut usize,
    connections: &InstanceGraph,
  ) -> bool {
    let mut context = Context::new();

    for id in active_instance_ids.iter() {
      let component_instance = instance_ids_to_instances.get_mut(&id).unwrap();

      let is_active = component_instance.step(&mut context);

      if !is_active {
        inactivate_ids.push(id.clone());
      }
    }

    for tmp in context.signal_requests {
      Orchestrator::signal_connector_out(
        tmp.node_index,
        tmp.instance_id,
        &connections,
        instance_ids_to_instances,
      );
    }

    for id in inactivate_ids.iter() {
      active_instance_ids.remove(id);
    }
    inactivate_ids.clear();
    *clock_cycle += 1;

    active_instance_ids.len() > 0
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
  // ) -> Option<&mut ComponentInstance> {
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
    match self.data.root_instance_id {
      Some(ref root_instance_id) => {
        let root_instance_id = root_instance_id.clone();
        self.signal_connector_in_on_instance(root_instance_id, node_index)
      }
      None => match self.data.root_component_name {
        Some(ref root_component_name) => {
          let root_component = self.data.components.get(root_component_name).unwrap();
          let root_component_instance =
            ComponentInstance::new(NodeName("root".to_string()), root_component, &[]);
          let root_component_instance_id = root_component_instance.id.clone();
          self.data.root_instance_id = Some(root_component_instance_id.clone());
          self
            .data
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
      .data
      .instance_ids_to_instances
      .get_mut(&instance_id)
      .unwrap();
    instance.signal_connector_in(node_index);
  }

  fn signal_connector_out(
    node_index: NodeIndex,
    from_instance_id: ComponentInstanceId,
    connections: &InstanceGraph,
    instance_ids_to_instances: &mut HashMap<ComponentInstanceId, ComponentInstance>,
  ) {
    if let Some((from_instance_node_index, _)) = connections
      .node_references()
      .find(|&(_, id)| id == &from_instance_id)
    {
      if let Some(edge) = connections
        .edges_directed(from_instance_node_index, Direction::Outgoing)
        .find(|edge| edge.weight().from_connector_index == node_index)
      {
        let to_instance_node_index = edge.weight().to_connector_index;
        let to_instance_id = connections[edge.target()].clone();
        let instance = instance_ids_to_instances.get_mut(&to_instance_id).unwrap();
        instance.signal_connector_in(to_instance_node_index);
      }
    }
  }
}

pub struct StepStatus {}

pub struct Stepper<'a> {
  data: &'a mut OrchestratorData,
}

// impl Stepper<'_> {
//   pub fn new(
//     orchestrator: &mut Orchestrator,
//     active_instance_ids: &mut HashSet<ComponentInstanceId>,
//   ) -> Self {
//     Self {
//       orchestrator,
//       active_instance_ids,
//     }
//   }
//}

impl Iterator for Stepper<'_> {
  type Item = StepStatus;

  fn next(&mut self) -> Option<Self::Item> {
    if Orchestrator::step(
      &mut self.data.active_instance_ids,
      &mut self.data.instance_ids_to_instances,
      &mut self.data.inactivate_ids,
      &mut self.data.clock_cycle,
      &self.data.connections,
    ) {
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

    let mut data = OrchestratorData::new();
    let mut orchestrator = Orchestrator::new(&mut data);
    let orchestrator = orchestrator.add_root_component(component);
    let orchestrator = orchestrator.signal_connector_in(connector_in);
    let orchestrator = orchestrator.run();

    //let stepper = Stepper::new(&mut orchestrator);

    assert_eq!(orchestrator.data.clock_cycle, 4);
  }
}
