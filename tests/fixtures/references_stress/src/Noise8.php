<?php

namespace App;

class Noise8
{
    private function compute(): int
    {
        return 8;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
