import { useRef, useMemo } from "react"
import { Canvas, useFrame } from "@react-three/fiber"
import { Float, MeshDistortMaterial } from "@react-three/drei"
import * as THREE from "three"

interface FingerprintMeshProps {
  scanning: boolean
  success: boolean
  error: boolean
}

function FingerprintMesh({ scanning, success, error }: FingerprintMeshProps) {
  const meshRef = useRef<THREE.Mesh>(null)
  const materialRef = useRef<any>(null)

  const color = useMemo(() => {
    if (success) return "#10B981"
    if (error) return "#EF4444"
    return "#8B5CF6"
  }, [success, error])

  useFrame((state) => {
    if (!meshRef.current) return

    meshRef.current.rotation.y = Math.sin(state.clock.elapsedTime * 0.3) * 0.2
    meshRef.current.rotation.x = Math.cos(state.clock.elapsedTime * 0.2) * 0.1

    if (scanning && materialRef.current) {
      const pulse = Math.sin(state.clock.elapsedTime * 4) * 0.5 + 0.5
      materialRef.current.distort = 0.3 + pulse * 0.3
    }
  })

  return (
    <Float speed={2} rotationIntensity={0.5} floatIntensity={0.5}>
      <mesh ref={meshRef} scale={scanning ? 1.1 : 1}>
        <torusKnotGeometry args={[1, 0.3, 128, 32]} />
        <MeshDistortMaterial
          ref={materialRef}
          color={color}
          envMapIntensity={0.5}
          clearcoat={1}
          clearcoatRoughness={0}
          metalness={0.5}
          roughness={0.2}
          distort={scanning ? 0.4 : 0.2}
          speed={scanning ? 4 : 2}
        />
      </mesh>
    </Float>
  )
}

function ParticleField({ count = 200 }) {
  const points = useMemo(() => {
    const positions = new Float32Array(count * 3)
    for (let i = 0; i < count; i++) {
      positions[i * 3] = (Math.random() - 0.5) * 10
      positions[i * 3 + 1] = (Math.random() - 0.5) * 10
      positions[i * 3 + 2] = (Math.random() - 0.5) * 10
    }
    return positions
  }, [count])

  const ref = useRef<THREE.Points>(null)

  useFrame((state) => {
    if (ref.current) {
      ref.current.rotation.y = state.clock.elapsedTime * 0.02
      ref.current.rotation.x = state.clock.elapsedTime * 0.01
    }
  })

  return (
    <points ref={ref}>
      <bufferGeometry>
        <bufferAttribute
          attach="attributes-position"
          count={count}
          array={points}
          itemSize={3}
        />
      </bufferGeometry>
      <pointsMaterial
        size={0.03}
        color="#8B5CF6"
        transparent
        opacity={0.6}
        sizeAttenuation
      />
    </points>
  )
}

interface Fingerprint3DProps {
  scanning?: boolean
  success?: boolean
  error?: boolean
  className?: string
}

export function Fingerprint3D({
  scanning = false,
  success = false,
  error = false,
  className = "",
}: Fingerprint3DProps) {
  return (
    <div className={`w-full h-full min-h-[200px] ${className}`}>
      <Canvas
        camera={{ position: [0, 0, 5], fov: 45 }}
        gl={{ antialias: true, alpha: true }}
      >
        <ambientLight intensity={0.5} />
        <pointLight position={[10, 10, 10]} intensity={1} />
        <pointLight position={[-10, -10, -10]} intensity={0.5} color="#8B5CF6" />
        <FingerprintMesh scanning={scanning} success={success} error={error} />
        <ParticleField />
      </Canvas>
    </div>
  )
}
