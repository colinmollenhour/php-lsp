<?php

namespace App;

class Noise22
{
    private function compute(): int
    {
        return 22;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
